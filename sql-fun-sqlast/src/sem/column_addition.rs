//! Completed additions and staged catalog effects, without mutating the analysis environment.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation, Column as AstColumn,
        ColumnConstraint as AstColumnConstraint, Dialect, ddl::AlterOperation,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, ColumnAttributeAction,
        ColumnDefinition, Constraint, ConstraintNames, CreateIndex, CreateTable, Kind, SemAst,
        SourcePathWithSpan,
        create_table::SerialSequences,
        problem::{AlterColumnProblem, CreationProblem},
    },
};

/// An ADD COLUMN request and its completed declaration/existence outcome.
#[derive(Debug, Clone)]
pub struct ColumnAddition {
    requested: AstColumn,
    definition: Option<ColumnDefinition>,
    existing: Option<ColumnDefinition>,
    skipped: bool,
    applicable: bool,
    constraints: Vec<Constraint>,
}

impl ColumnAddition {
    pub(crate) fn is_ast_relation(ast: &AstAlterRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| Self::ast_command(command).is_some())
    }

    pub(crate) fn analyze_relation(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ADD COLUMN source");
        };
        let mut analysis = AdditionAnalysis::new(ast, context, source)?;
        analysis.analyze_attributes(ast, context, true)?;
        analysis.declare_columns(ast, context)?;
        analysis.resolve_table_constraints(ast, context)?;
        analysis.resolve_column_constraints(context)?;
        analysis.resolve_checks(context)?;
        analysis.analyze_attributes(ast, context, false)?;
        analysis.finish(context, source)
    }

    pub(crate) fn analyze_single(
        context: &mut impl ParseContext,
        table: &TableName,
        syntax: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        let ast = AstAlterRelation::single_column_addition(table.full_name(), syntax);
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("ADD COLUMN source");
        };
        let statement = Self::analyze_relation(&ast, context, Some(&source))?;
        Ok(statement
            .alter_table_statement()
            .expect("analyzed ALTER")
            .commands()[0]
            .column_addition()
            .expect("analyzed ADD")
            .clone())
    }

    #[expect(clippy::too_many_arguments)]
    fn declare(
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        syntax: &AstColumn,
        if_not_exists: bool,
        skip_missing: bool,
        serials: &mut SerialSequences,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(syntax.name().as_str());
        let existing = if let Some(definition) = staged {
            definition
                .find_column(&name, context.dialect() == Dialect::sqlite())
                .cloned()
        } else {
            None
        };
        if existing.is_some() || staged.is_none() {
            let skipped =
                (existing.is_some() && if_not_exists) || (staged.is_none() && skip_missing);
            if existing.is_some() && !skipped {
                context.report_analysis_problem(AnalysisProblem::column_already_exists(
                    table, &name,
                ))?;
            }
            return Ok(Self::new(syntax, None, existing.as_ref(), skipped, skipped));
        }
        Self::check_ast_support(syntax);
        // Shared SQLite declarations still require catalog type names (#1069).
        let declaration = syntax.without_foreign_keys();
        let temporary = staged.as_ref().expect("resolved target").temporary();
        let mut definition =
            ColumnDefinition::from_ast_serial(&declaration, context, table, temporary, serials)?;
        if context.dialect() == Dialect::postgres() {
            definition.apply_primary_key_nullability();
        }
        let type_resolved = syntax.type_name().is_none() || definition.get_type().is_some();
        let mut result = Self::new(syntax, Some(&definition), None, false, type_resolved);
        result.validate_keys(context, table, staged.as_ref().expect("resolved target"))?;
        staged
            .as_mut()
            .expect("resolved target")
            .append_declared_column(result.definition.as_ref().expect("declared column"));
        Ok(result)
    }
}

#[cfg(test)]
mod declaration_tests {
    use crate::{ast::Dialect, test_helpers::checks};

    #[test]
    fn an_existing_column_skip_does_not_resolve_the_requested_type() -> testresult::TestResult {
        let context = checks::apply(
            checks::context(Dialect::postgres()),
            "CREATE TABLE t(a integer)",
        )?
        .into_context();
        let pair = checks::apply(
            context,
            "ALTER TABLE t ADD COLUMN IF NOT EXISTS a no_such_type, ADD COLUMN b integer",
        )?;
        let statement = pair.ast().alter_table_statement().unwrap();
        assert!(statement.can_apply());
        let addition = statement.commands()[0].column_addition().unwrap();
        assert!(addition.is_skipped());
        assert!(addition.existing().is_some());
        assert!(addition.definition().is_none());
        assert!(
            pair.context().problems().is_empty(),
            "{:?}",
            pair.context().problems()
        );
        Ok(())
    }
}

impl ColumnAddition {
    fn check_ast_support(syntax: &AstColumn) {
        for definition in syntax.constraints() {
            match definition.constraint() {
                AstColumnConstraint::Generated(..) => {
                    todo!("ADD COLUMN GENERATED (#1060; row-state analysis #1062)")
                }
                AstColumnConstraint::Identity(_) => todo!("ADD COLUMN IDENTITY (#1061)"),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod ast_support_tests {
    use crate::{ast::Dialect, test_helpers::checks};

    #[rstest::rstest]
    #[case::generated("ALTER TABLE t ADD COLUMN b integer GENERATED ALWAYS AS(a + 1) STORED")]
    #[case::identity("ALTER TABLE t ADD COLUMN b bigint GENERATED ALWAYS AS IDENTITY")]
    #[should_panic(expected = "ADD COLUMN")]
    fn deferred_declarations_cannot_be_published_as_ordinary_columns(#[case] sql: &str) {
        let context = checks::apply(
            checks::context(Dialect::postgres()),
            "CREATE TABLE t(a integer)",
        )
        .unwrap()
        .into_context();
        checks::apply(context, sql).unwrap();
    }
}

impl ColumnAddition {
    fn validate_keys(
        &mut self,
        context: &mut impl ParseContext,
        table: &TableName,
        pending: &CreateTable,
    ) -> Result<(), AnalysisError> {
        self.validate_nullability(context, table)?;
        let mut has_primary_key = !pending.primary_key_columns().is_empty();
        for constraint in self.requested.constraints() {
            let primary = matches!(constraint.constraint(), AstColumnConstraint::PrimaryKey(..));
            let reason = if constraint.constraint().is_unique_key()
                && context.dialect() == Dialect::sqlite()
            {
                Some(AlterColumnProblem::add_sqlite_key())
            } else if primary && has_primary_key {
                Some(AlterColumnProblem::add_primary_key())
            } else {
                None
            };
            has_primary_key |= primary;
            if let Some(reason) = reason {
                self.applicable = false;
                context.report_problem(AnalysisProblem::alter_column(
                    table,
                    &self.name(),
                    &reason,
                ))?;
            }
        }
        Ok(())
    }

    fn validate_nullability(
        &mut self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<(), AnalysisError> {
        let mut nullable = false;
        let mut not_null = false;
        for definition in self.requested.constraints() {
            match definition.constraint() {
                AstColumnConstraint::Nullability(false, ..) => nullable = true,
                AstColumnConstraint::Nullability(true, ..)
                | AstColumnConstraint::PrimaryKey(..) => not_null = true,
                _ => {}
            }
        }
        if context.dialect() == Dialect::sqlite() {
            // SQLite NULL does not cancel an explicit NOT NULL declaration.
            if not_null && let Some(column) = &mut self.definition {
                column.set_not_null(Some(true));
            }
            return Ok(());
        }
        if nullable && not_null {
            self.applicable = false;
            context.report_problem(AnalysisProblem::alter_column(
                table,
                &self.name(),
                &AlterColumnProblem::add_nullability(),
            ))?;
        }
        Ok(())
    }

    pub(crate) fn new(
        requested: &AstColumn,
        definition: Option<&ColumnDefinition>,
        existing: Option<&ColumnDefinition>,
        skipped: bool,
        applicable: bool,
    ) -> Self {
        Self {
            requested: requested.clone(),
            definition: definition.cloned(),
            existing: existing.cloned(),
            skipped,
            applicable,
            constraints: Vec::new(),
        }
    }
    /// Column name explicitly requested by the statement.
    pub fn name(&self) -> ColumnName {
        ColumnName::from(self.requested.name().as_str())
    }
    /// Analyzed declaration; skipped requests deliberately do not analyze it.
    pub const fn definition(&self) -> Option<&ColumnDefinition> {
        self.definition.as_ref()
    }
    /// Existing column captured when the requested name was occupied.
    pub const fn existing(&self) -> Option<&ColumnDefinition> {
        self.existing.as_ref()
    }
    /// Whether an explicit existence clause made this request a no-op.
    pub const fn is_skipped(&self) -> bool {
        self.skipped
    }
    /// Whether this request permits application of its enclosing statement.
    pub fn can_apply(&self) -> bool {
        self.applicable
            && self
                .definition
                .as_ref()
                .is_none_or(ColumnDefinition::declaration_can_apply)
            && self
                .constraints
                .iter()
                .all(Constraint::references_can_apply)
    }
    /// Resolved key/reference declarations belonging to this added column.
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub(crate) fn apply_to(&self, table: &mut CreateTable) -> Result<(), AnalysisError> {
        if let Some(column) = &self.definition {
            table.apply_added_column(column, &self.constraints)?;
        }
        Ok(())
    }

    pub(crate) fn ast_command(ast: &AstAlterCommand) -> Option<(&AstColumn, bool)> {
        match ast {
            AstAlterCommand::AddColumn(column) => Some((column, false)),
            AstAlterCommand::Postgres(command)
                if matches!(command.operation(), AlterOperation::AddColumn) =>
            {
                command
                    .definition()
                    .as_column()
                    .map(|column| (column, command.missing_ok()))
            }
            _ => None,
        }
    }
}

// This pass state owns the prospective row and pending effects. ParseContext
// remains the shared environment; nothing is published before applicability is final.
struct AdditionAnalysis {
    requested: TableName,
    target: Option<TableName>,
    staged: Option<CreateTable>,
    names: ConstraintNames,
    additions: Vec<ColumnAddition>,
    commands: Vec<AlterObjSubCommand>,
    serials: SerialSequences,
    indexes: Vec<CreateIndex>,
    applicable: bool,
    skip_missing: bool,
    source: SourcePathWithSpan,
}
impl AdditionAnalysis {
    fn new(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<Self, AnalysisError> {
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested).map(|name| {
            if context.dialect() == Dialect::sqlite() {
                TableName::from(name.full_name().sqlite_identity())
            } else {
                name
            }
        });
        let staged = if let Some(name) = &target {
            context
                .get_table_impl(name.full_name())
                .map(|definition| definition.for_alter_analysis(name))
        } else {
            None
        };
        let skip_missing = target.is_none()
            && ast.if_exists()
            && context
                .resolve_relation_name(requested.full_name())
                .is_none();
        if target.is_none() && !skip_missing {
            context.report_problem(AnalysisProblem::relation_not_found(&requested))?;
        }
        let mut names = ConstraintNames::default();
        if let Some(table) = &staged {
            table.reserve_constraint_names(&mut names);
        }
        let applicable = target.is_some();
        Ok(Self {
            requested,
            target,
            staged,
            names,
            additions: Vec::new(),
            commands: Vec::new(),
            serials: SerialSequences::default(),
            indexes: Vec::new(),
            applicable,
            skip_missing,
            source: source.clone(),
        })
    }
    fn table(&self) -> &TableName {
        self.target.as_ref().unwrap_or(&self.requested)
    }

    fn analyze_attributes(
        &mut self,
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        before_addition: bool,
    ) -> Result<(), AnalysisError> {
        let mut ordered: Vec<_> = ast
            .commands()
            .iter()
            .filter(|command| {
                ColumnAttributeAction::phase_from_ast_command(command)
                    .is_some_and(|phase| (phase == 0) == before_addition)
            })
            .collect();
        ordered.sort_by_key(|command| ColumnAttributeAction::phase_from_ast_command(command));
        let table = self.table().clone();
        for command in ordered {
            self.commands
                .push(AlterObjSubCommand::from_ast_column_attribute(
                    command,
                    context,
                    &table,
                    &mut self.staged,
                    self.skip_missing,
                )?);
        }
        Ok(())
    }

    fn declare_columns(
        &mut self,
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let table = self.table().clone();
        for command in ast.commands() {
            let Some((syntax, if_not_exists)) = ColumnAddition::ast_command(command) else {
                if self.skip_missing
                    || ColumnAttributeAction::phase_from_ast_command(command).is_some()
                    || Constraint::added_ast_constraint(command).is_some()
                {
                    continue;
                }
                todo!(
                    "ADD COLUMN mixed with other ALTER operations (#1063; row-state analysis #1062)"
                );
            };
            let addition = ColumnAddition::declare(
                context,
                &table,
                &mut self.staged,
                syntax,
                if_not_exists,
                self.skip_missing,
                &mut self.serials,
            )?;
            if addition.definition().is_some() {
                ConstraintNames::reserve_ast_column(syntax, context, &mut self.names);
            }
            self.additions.push(addition);
        }
        if let Some(name) = self.names.first_duplicate() {
            self.applicable = false;
            context.report_problem(AnalysisProblem::creation(CreationProblem::constraint(
                table.full_name(),
                &FullName::from_local_name(name),
                self.source.span(),
            )))?;
        }
        Ok(())
    }

    fn resolve_column_constraints(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let Some(pending) = &mut self.staged else {
            return Ok(());
        };
        // Keys precede REFERENCES so self references may bind later new keys.
        for addition in &mut self.additions {
            if addition.definition.is_none() {
                continue;
            }
            for definition in addition.requested.constraints() {
                if definition.constraint().is_unique_key() && definition.name().is_some() {
                    let constraint = Constraint::from_ast_column_table(
                        definition.constraint(),
                        context,
                        pending.name(),
                        definition.name().as_deref().unwrap_or_default(),
                        &[addition.name()],
                    )?;
                    if pending.constraint(constraint.name()).is_none() {
                        pending.record_declared_constraint(&constraint)?;
                    }
                    addition.constraints.push(constraint);
                }
            }
        }
        for addition in &mut self.additions {
            if addition.definition.is_none() {
                continue;
            }
            for definition in addition.requested.constraints() {
                if let Some(mut constraint) = Constraint::from_ast_column_foreign_key(
                    definition.constraint(),
                    context,
                    pending,
                    definition.name().as_deref().unwrap_or_default(),
                    &[addition.name()],
                )? {
                    if context.dialect() == Dialect::postgres() {
                        constraint.validate_added_column_types(context, pending)?;
                    }
                    addition.constraints.push(constraint);
                }
            }
        }
        Ok(())
    }

    fn resolve_checks(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        let table = self.table().clone();
        for addition in &mut self.additions {
            if let Some(column) = &mut addition.definition {
                let declaration = addition.requested.without_foreign_keys();
                ColumnDefinition::resolve_ast_inline_checks(
                    &declaration,
                    context,
                    &table,
                    self.staged.as_ref(),
                    &mut self.names,
                    column,
                )?;
                if SerialSequences::integer_for_ast_column(&addition.requested, context).is_none() {
                    column.validate_added_default(context, &table)?;
                }
            }
            self.commands.push(AlterObjSubCommand::add_column(addition));
        }
        Ok(())
    }

    fn resolve_table_constraints(
        &mut self,
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.staged.is_none() {
            return Ok(());
        }
        let table = self.table().clone();
        let mut constraints: Vec<_> = ast
            .commands()
            .iter()
            .filter_map(Constraint::added_ast_constraint)
            .collect();
        constraints.sort_by_key(|syntax| !syntax.constraint().is_unique_key());
        for syntax in constraints {
            let (constraint, valid) = Constraint::from_ast_added_constraint(
                syntax,
                context,
                &table,
                self.staged.as_ref(),
            )?;
            let duplicate = !self.names.reserve(constraint.name());
            self.applicable &= valid && !duplicate;
            if duplicate && valid {
                context.report_problem(AnalysisProblem::creation(CreationProblem::constraint(
                    table.full_name(),
                    &FullName::from_local_name(constraint.name()),
                    self.source.span(),
                )))?;
            }
            if valid
                && !duplicate
                && let Some(pending) = &mut self.staged
            {
                if constraint.is_primary_key_constraint()
                    && !pending.primary_key_columns().is_empty()
                {
                    self.applicable = false;
                    let column = constraint.first_key().expect("primary key column");
                    context.report_problem(AnalysisProblem::alter_column(
                        &table,
                        column,
                        &AlterColumnProblem::add_primary_key(),
                    ))?;
                } else {
                    pending.record_declared_constraint(&constraint)?;
                }
            }
            self.commands
                .push(AlterObjSubCommand::add_constraint(&constraint));
        }
        Ok(())
    }

    fn finish(
        mut self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<SemAst, AnalysisError> {
        let sequences = self.serials.into_sequences();
        let table = self.target.as_ref().unwrap_or(&self.requested);
        for addition in &self.additions {
            for constraint in &addition.constraints {
                self.indexes.extend(constraint.addition_indexes(table)?);
            }
        }
        for command in &self.commands {
            self.indexes.extend(command.addition_indexes(table)?);
        }
        for (position, index) in self.indexes.iter().enumerate() {
            self.applicable &=
                context.permits_relation_creation(index.name(), Kind::index(), false, false)?;
            if CreateIndex::contains_name(&self.indexes[..position], index.name()) {
                self.applicable = false;
                context.report_problem(AnalysisProblem::creation(CreationProblem::relation(
                    index.name(),
                    Kind::index(),
                    Kind::index(),
                    source.span(),
                )))?;
            }
        }
        if let Some(table) = &mut self.staged {
            table.retain_serial_sequences(sequences.clone());
            self.applicable &= table.permits_serial_creation(context, &self.indexes)?;
        }
        Ok(SemAst::alter_table(&AlterTable::column_additions(
            &self.requested,
            self.target.as_ref(),
            &self.commands,
            source,
            self.applicable,
            &self.indexes,
            &sequences,
        )))
    }
}

#[cfg(test)]
mod schema_chain_tests {
    //! Integration contracts between completed ADD analysis and layered catalog application.

    use crate::{
        ast::Dialect,
        context::{BaseParseContext, ParseContext},
        names::{ColumnName, FullName, TableName},
        sem::{AlterTable, PgBuiltInType},
        test_helpers::{checks, public_analysis, schema_layer},
    };

    #[test]
    fn addition_updates_later_queries_and_preserves_the_base_layer() -> testresult::TestResult {
        let base = checks::apply(
            checks::context(Dialect::postgres()),
            "CREATE TABLE t(a integer)",
        )?
        .into_context()
        .freeze();
        let context = schema_layer(std::path::Path::new("add.sql"), &base);
        let pair = checks::apply(
            context,
            "ALTER TABLE t ADD COLUMN b integer NOT NULL DEFAULT 1",
        )?;
        let statement = pair.ast().alter_table_statement().unwrap();
        assert!(statement.can_apply());
        let addition = statement.commands()[0].column_addition().unwrap();
        assert!(addition.can_apply());
        assert!(!addition.is_skipped());
        assert_eq!(
            addition.definition().unwrap().get_type(),
            Some(&PgBuiltInType::int4())
        );
        let context = pair.into_context();
        let name = TableName::from(FullName::with_schema("public", "t"));
        assert!(
            !base
                .get_table(&name)
                .unwrap()
                .has_column(&ColumnName::from("b"))
        );
        assert_eq!(
            context.get_table(&name).unwrap().alter_statements().len(),
            1
        );
        let pair = checks::apply(context, "SELECT b FROM t")?;
        assert!(
            pair.context().problems().is_empty(),
            "{:?}",
            pair.context().problems()
        );
        let column = pair
            .ast()
            .select_statement()
            .unwrap()
            .get_result_column_def(&ColumnName::from("b"))
            .unwrap();
        assert_eq!(column.get_type(), Some(&PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), Some(true));
        Ok(())
    }

    #[rstest::rstest]
    #[case::add_then_attribute(
        "ALTER TABLE t ADD COLUMN b integer, ALTER COLUMN b SET NOT NULL",
        true
    )]
    #[case::drop_before_add(
        "ALTER TABLE t ADD COLUMN b integer, ALTER COLUMN b DROP NOT NULL",
        false
    )]
    #[case::forward_check(
        "ALTER TABLE t ADD COLUMN b integer CHECK(b < c), ADD COLUMN c integer",
        true
    )]
    #[case::missing_reference(
        "ALTER TABLE t ADD COLUMN b integer, ADD COLUMN c integer CHECK(c < absent)",
        false
    )]
    #[case::duplicate_column("ALTER TABLE t ADD COLUMN b integer, ADD COLUMN b integer", false)]
    #[case::existing_column("ALTER TABLE t ADD COLUMN b integer, ADD COLUMN a integer", false)]
    fn a_group_publishes_all_effective_columns_or_none(
        #[case] sql: &str,
        #[case] applicable: bool,
    ) -> testresult::TestResult {
        let context = checks::apply(
            checks::context(Dialect::postgres()),
            "CREATE TABLE t(a integer)",
        )?
        .into_context();
        let statement = public_analysis::statement(sql, Dialect::postgres());
        let pair = AlterTable::analyze(&statement, context)?;
        assert_eq!(
            pair.ast().alter_table_statement().unwrap().can_apply(),
            applicable
        );
        let (ast, context) = (pair.ast().clone(), pair.into_context());
        let context = schema_layer(std::path::Path::new("apply.sql"), &context.freeze());
        assert!(context.problems().is_empty());
        let context = ast.apply_relation_alter(context)?;
        let table = context
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        assert_eq!(table.has_column(&ColumnName::from("b")), applicable);
        assert_eq!(table.alter_statements().len(), usize::from(applicable));
        assert!(
            checks::apply(context, "SELECT a FROM t")?
                .context()
                .problems()
                .is_empty()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing_type("ALTER TABLE t ADD COLUMN b no_such_type", false)]
    #[case::invalid_default("ALTER TABLE t ADD COLUMN b boolean DEFAULT 1", false)]
    #[case::missing_fk("ALTER TABLE t ADD COLUMN b integer REFERENCES absent(id)", false)]
    #[case::forward_table_check("ALTER TABLE t ADD COLUMN b integer, ADD CHECK(a < b)", true)]
    #[case::duplicate_inline_check(
        "ALTER TABLE t ADD COLUMN b integer CONSTRAINT repeated CHECK(true), ADD COLUMN c integer CONSTRAINT repeated CHECK(true)",
        false
    )]
    #[case::duplicate_key_name(
        "ALTER TABLE t ADD COLUMN b integer CONSTRAINT repeated UNIQUE, ADD COLUMN c integer CONSTRAINT repeated UNIQUE",
        false
    )]
    #[case::conflicting_nullability("ALTER TABLE t ADD COLUMN b integer PRIMARY KEY NULL", false)]
    #[case::repeated_primary_key(
        "ALTER TABLE t ADD COLUMN b integer PRIMARY KEY PRIMARY KEY",
        false
    )]
    #[case::table_primary_key("ALTER TABLE t ADD COLUMN b integer, ADD PRIMARY KEY(b)", true)]
    #[case::forward_self_reference(
        "ALTER TABLE t ADD COLUMN b integer REFERENCES t, ADD COLUMN c integer PRIMARY KEY",
        true
    )]
    #[case::second_primary_key(
        "ALTER TABLE t ADD COLUMN b integer PRIMARY KEY, ADD COLUMN c integer PRIMARY KEY",
        false
    )]
    fn declaration_failures_are_retained_and_atomic(
        #[case] sql: &str,
        #[case] applicable: bool,
    ) -> testresult::TestResult {
        let context = checks::apply(
            checks::context(Dialect::postgres()),
            "CREATE TABLE t(a integer)",
        )?
        .into_context();
        let pair = checks::apply(context, sql)?;
        assert_eq!(
            pair.ast().alter_table_statement().unwrap().can_apply(),
            applicable
        );
        let table = pair
            .context()
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        assert_eq!(table.has_column(&ColumnName::from("b")), applicable);
        assert_eq!(table.alter_statements().len(), usize::from(applicable));
        Ok(())
    }

    #[test]
    fn references_retain_the_selected_relation_and_implicit_indexes_are_atomic()
    -> testresult::TestResult {
        use crate::{
            context::{IndexInfoRead, TableInfoRead},
            test_helpers::foreign_keys as fk,
        };
        let context = fk::apply(fk::parents(), "CREATE TABLE a.t(a integer)");
        let pair = checks::apply(
            context,
            "ALTER TABLE a.t ADD COLUMN b integer CONSTRAINT added_reference REFERENCES parent, ADD COLUMN c integer CONSTRAINT added_key UNIQUE",
        )?;
        assert!(
            pair.context().problems().is_empty(),
            "{:?}",
            pair.context().problems()
        );
        assert!(pair.ast().alter_table_statement().unwrap().can_apply());
        let first = pair.ast().alter_table_statement().unwrap().commands()[0]
            .column_addition()
            .unwrap();
        assert_eq!(
            first.constraints()[0]
                .as_foreign_key()
                .unwrap()
                .target_table(),
            Some(&fk::name("a", "parent"))
        );
        assert!(
            pair.context()
                .get_index_impl(&FullName::with_schema("a", "added_key"))
                .is_some()
        );
        let mut context = pair.into_context();
        fk::reversed_path(&mut context);
        let table = context
            .get_table_impl(&FullName::with_schema("a", "t"))
            .unwrap();
        assert_eq!(
            table
                .constraint("added_reference")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .target_table(),
            Some(&fk::name("a", "parent"))
        );
        let pair = checks::apply(
            context,
            "ALTER TABLE a.t ADD COLUMN d integer, ADD COLUMN e integer CONSTRAINT added_key UNIQUE",
        )?;
        assert!(!pair.ast().alter_table_statement().unwrap().can_apply());
        assert!(
            !pair
                .context()
                .get_table_impl(&FullName::with_schema("a", "t"))
                .unwrap()
                .has_column(&ColumnName::from("d"))
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::success("ALTER TABLE t ADD COLUMN b bigserial", true)]
    #[case::failed_check(
        "ALTER TABLE t ADD COLUMN b bigserial, ADD COLUMN c integer CHECK(absent > 0)",
        false
    )]
    fn serial_sequences_follow_statement_applicability(
        #[case] sql: &str,
        #[case] applicable: bool,
    ) -> testresult::TestResult {
        use crate::{context::SequenceInfoRead, test_helpers::serial};
        let context = checks::apply(serial::context(), "CREATE TABLE t(a integer)")?.into_context();
        let pair = checks::apply(context, sql)?;
        assert_eq!(
            pair.ast().alter_table_statement().unwrap().can_apply(),
            applicable,
            "{:?}",
            pair.context().problems()
        );
        assert_eq!(
            pair.context()
                .get_sequence_impl(&FullName::with_schema("public", "t_b_seq"))
                .is_some(),
            applicable
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::ordinary("ALTER TABLE t ADD COLUMN b integer CHECK(b)", true)]
    #[case::duplicate_case("ALTER TABLE t ADD COLUMN A integer", false)]
    #[case::missing_type("ALTER TABLE t ADD COLUMN b arbitrary_type_name", false)]
    #[case::typeless("ALTER TABLE t ADD COLUMN b", true)]
    #[case::null_then_not_null("ALTER TABLE t ADD COLUMN b integer NULL NOT NULL", true)]
    #[case::not_null_then_null("ALTER TABLE t ADD COLUMN b integer NOT NULL NULL", true)]
    fn sqlite_additions_follow_their_own_identifier_and_key_rules(
        #[case] sql: &str,
        #[case] applicable: bool,
    ) -> testresult::TestResult {
        let context = checks::apply(
            checks::context(Dialect::sqlite()),
            "CREATE TABLE t(a integer)",
        )?
        .into_context();
        let pair = checks::apply(context, sql)?;
        let statement = pair.ast().alter_table_statement().unwrap();
        assert_eq!(
            statement.can_apply(),
            applicable,
            "{:?}",
            pair.context().problems()
        );
        let table = pair
            .context()
            .get_table(statement.resolved_target().unwrap())
            .unwrap();
        assert_eq!(table.has_column(&ColumnName::from("b")), applicable);
        if sql.contains("NOT NULL") {
            assert_eq!(
                table
                    .find_column(&ColumnName::from("b"), true)
                    .unwrap()
                    .is_not_null(),
                Some(true)
            );
        }
        Ok(())
    }
}
