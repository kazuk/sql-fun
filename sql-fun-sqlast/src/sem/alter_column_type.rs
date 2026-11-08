//! Analyzed column type requests and assignment conversions, without catalog writes.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation, Expr as AstExpr,
        ddl::{AlterColumnType as AstAlterColumnType, AlterDefinition as AstAlterDefinition},
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, CastContext,
        ColumnAttributeAction, ColumnDefinition, CommonType, CreateTable, DeclarationCollation,
        ScalarAnalysisContext, SemAst, SemScalarExpr, SourcePathWithSpan, TypeReference,
        WithTypmod, problem::AlterColumnProblem, traversal::SemanticVisitor,
    },
};

/// A completed assignment conversion with its analyzed operand and outcome.
#[derive(Clone, Debug)]
pub struct ColumnTypeConversion {
    expression: Option<SemScalarExpr>,
    source: Option<TypeReference>,
    target: Option<TypeReference>,
    conversion: Option<CastContext>,
}
impl ColumnTypeConversion {
    fn analyze(
        context: &impl ParseContext,
        expression: Option<&SemScalarExpr>,
        source: Option<&TypeReference>,
        target: Option<&TypeReference>,
    ) -> Self {
        let conversion = match (source, target) {
            (Some(source), Some(target)) if source == target => Some(CastContext::no_conversion()),
            (Some(source), Some(_)) if source == &CommonType::unknown() => {
                Some(CastContext::implicit())
            }
            (Some(source), Some(target)) => source.assignment_match_context(target, context),
            _ => None,
        };
        Self {
            expression: expression.cloned(),
            source: source.cloned(),
            target: target.cloned(),
            conversion,
        }
    }
    fn column_default(
        context: &impl ParseContext,
        column: &ColumnDefinition,
        target: Option<&TypeReference>,
    ) -> Option<Self> {
        let expression = column.default_expression()?;
        let mut source = SemScalarExpr::common_type_input(expression);
        // A string default was already typed in the original column's
        // context. USING cannot reinterpret it as a fresh unknown literal.
        // A bare NULL default remains NULL; an explicit COLLATE wrapper
        // retains the original declaration context.
        if source.as_ref() == Some(&CommonType::unknown())
            && !expression
                .as_const()
                .is_some_and(|literal| literal.is_null())
        {
            source = column.get_type_cloned();
        }
        Some(Self::analyze(
            context,
            Some(expression),
            source.as_ref(),
            target,
        ))
    }
    /// Written conversion/default expression; absent for implicit column conversion.
    pub const fn expression(&self) -> Option<&SemScalarExpr> {
        self.expression.as_ref()
    }
    /// Source type before assignment conversion.
    pub const fn source(&self) -> Option<&TypeReference> {
        self.source.as_ref()
    }
    /// Canonical requested destination type, absent after failed type lookup.
    pub const fn target(&self) -> Option<&TypeReference> {
        self.target.as_ref()
    }
    /// Completed assignment-conversion outcome; None is not successful conversion.
    pub const fn conversion(&self) -> Option<CastContext> {
        self.conversion
    }
    /// Whether the operand can be assigned to the requested type.
    pub const fn can_assign(&self) -> bool {
        self.conversion.is_some()
    }
}

#[cfg(test)]
mod default_conversion_tests {
    use crate::{
        ast::Dialect,
        sem::AlterTable,
        test_helpers::{alter_column_type, public_analysis},
    };

    #[rstest::rstest]
    #[case::null("NULL", true)]
    #[case::collated_string("'true' COLLATE \"C\"", false)]
    #[case::collated_null("NULL COLLATE \"C\"", false)]
    fn default_context_preserves_existing_coercions(
        #[case] default: &str,
        #[case] valid: bool,
    ) -> testresult::TestResult {
        let context = alter_column_type::declared_default(default)?;
        let statement = public_analysis::statement(
            "ALTER TABLE defaults ALTER COLUMN a TYPE boolean USING true",
            Dialect::postgres(),
        );
        let result = AlterTable::analyze(&statement, context)?;
        let change = result.ast().alter_table_statement().unwrap().commands()[0]
            .column_type_change()
            .unwrap();
        assert_eq!(change.default().unwrap().can_assign(), valid);
        assert_eq!(change.is_valid(), valid);
        Ok(())
    }
}

/// Completed analysis of one ALTER COLUMN TYPE, retaining rejected requests too.
#[derive(Clone, Debug)]
pub struct AlterColumnType {
    table: TableName,
    column: ColumnName,
    original: Option<ColumnDefinition>,
    requested: WithTypmod<TypeReference>,
    collation: DeclarationCollation,
    value: ColumnTypeConversion,
    default: Option<ColumnTypeConversion>,
    problems: Vec<AlterColumnProblem>,
    skipped: bool,
}
impl AlterColumnType {
    pub(crate) fn from_ast(
        ast: &AstAlterColumnType,
        context: &mut impl ParseContext,
        table: &TableName,
        definition: Option<&CreateTable>,
        column: &ColumnName,
        duplicate: bool,
        skipped: bool,
    ) -> Result<Self, AnalysisError> {
        let original = if let Some(table) = definition {
            table.get_column_def(column).cloned()
        } else {
            None
        };
        let spelling = TypeReference::from_ast(ast.type_name())?;
        let destination = if skipped {
            None
        } else {
            context.canonicalize_type_ref(&spelling)
        };
        let requested = WithTypmod::from_ast_declaration(ast.type_name(), destination.as_ref())?;
        let lookup = ast
            .collation()
            .as_ref()
            .map(FullName::from_ast_name)
            .transpose()?
            .filter(|_| !skipped)
            .map(|name| context.lookup_collation(&name));
        let collation =
            DeclarationCollation::validate(context, destination.as_ref(), lookup.as_ref());
        let expression = if skipped {
            None
        } else {
            Self::using_from_ast(ast.using().as_ref(), context, table, definition)?
        };
        let source = if let Some(expression) = &expression {
            SemScalarExpr::common_type_input(expression)
        } else if let Some(column) = &original {
            column.get_type_cloned()
        } else {
            None
        };
        let value = ColumnTypeConversion::analyze(
            context,
            expression.as_ref(),
            source.as_ref(),
            destination.as_ref(),
        );
        let default = if let Some(column) = &original {
            ColumnTypeConversion::column_default(context, column, destination.as_ref())
        } else {
            None
        };
        let mut result = Self {
            table: table.clone(),
            column: column.clone(),
            original,
            requested,
            collation,
            value,
            default,
            problems: Vec::new(),
            skipped,
        };
        if !skipped {
            result.validate_target(definition, duplicate);
            result.validate_definition(context, &spelling)?;
            result.validate_conversion(context)?;
        }
        for problem in &result.problems {
            context
                .report_analysis_problem(AnalysisProblem::alter_column(table, column, problem))?;
        }
        Ok(result)
    }
    fn validate_target(&mut self, definition: Option<&CreateTable>, duplicate: bool) {
        if definition.is_none() {
            self.problems.push(AlterColumnProblem::missing_relation());
        } else if self.original.is_none() {
            self.problems.push(AlterColumnProblem::missing_column());
        }
        if duplicate {
            self.problems.push(AlterColumnProblem::duplicate_type());
        }
    }
    fn validate_definition(
        &mut self,
        context: &mut impl ParseContext,
        spelling: &TypeReference,
    ) -> Result<(), AnalysisError> {
        if self.value.target().is_none() {
            context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                &self.table,
                &self.column,
                spelling.full_name().expect("written type identity"),
            ))?;
        }
        if let Some(message) = self.requested.validation_error() {
            self.problems
                .push(AlterColumnProblem::invalid_typmod(message));
        }
        self.collation.report(context, self.value.target())
    }
    fn validate_conversion(&mut self, context: &impl ParseContext) -> Result<(), AnalysisError> {
        if self.original.is_some() && self.value.target().is_some() && !self.value.can_assign() {
            self.problems.push(AlterColumnProblem::type_conversion());
        }
        if self
            .default
            .as_ref()
            .is_some_and(|value| !value.can_assign())
        {
            self.problems.push(AlterColumnProblem::default_conversion());
        }
        if let Some(expression) = self.value.expression() {
            let mut validator = UsingExpression {
                context,
                valid: true,
            };
            expression.visit(&mut validator)?;
            if !validator.valid {
                self.problems.push(AlterColumnProblem::using_expression());
            }
        }
        Ok(())
    }
    fn using_from_ast(
        ast: Option<&AstExpr>,
        context: &mut impl ParseContext,
        table: &TableName,
        definition: Option<&CreateTable>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let scope = if let Some(definition) = definition {
            ScalarAnalysisContext::declared_table(definition, table)
        } else {
            ScalarAnalysisContext::default()
        };
        ast.map(|expression| SemScalarExpr::from_ast_in(expression, context, &scope))
            .transpose()
    }
    /// Canonical target relation, or requested identity when it was not found.
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// Requested column identity.
    pub const fn column(&self) -> &ColumnName {
        &self.column
    }
    /// Completed original-column binding, absent after failed lookup.
    pub const fn original(&self) -> Option<&ColumnDefinition> {
        self.original.as_ref()
    }
    /// Written target type and interpreted modifiers.
    pub const fn requested(&self) -> &WithTypmod<TypeReference> {
        &self.requested
    }
    /// Explicit declaration collation lookup and applicability.
    pub const fn collation(&self) -> &DeclarationCollation {
        &self.collation
    }
    /// Original-row value conversion, including the analyzed USING operand.
    pub const fn value(&self) -> &ColumnTypeConversion {
        &self.value
    }
    /// Independent default conversion; absent when the column has no default.
    pub const fn default(&self) -> Option<&ColumnTypeConversion> {
        self.default.as_ref()
    }
    /// Retained structured rejection reasons, independent of diagnostic delivery.
    pub fn problems(&self) -> &[AlterColumnProblem] {
        &self.problems
    }
    /// Whether IF EXISTS skipped a missing relation.
    pub const fn is_skipped(&self) -> bool {
        self.skipped
    }
    /// Whether target, declaration and local conversion analysis succeeded.
    ///
    /// Dependencies are not validated here. Schema application (#1057) may
    /// reject a locally valid operation after examining dependent definitions.
    pub fn is_valid(&self) -> bool {
        !self.skipped
            && self.original.is_some()
            && self.value.can_assign()
            && self.collation.can_apply()
            && self.requested.validation_error().is_none()
            && self.problems.is_empty()
    }
}

#[cfg(test)]
mod analysis_tests {
    use crate::{
        ast::Dialect,
        context::BaseParseContext,
        names::{ColumnName, FullName, TableName},
        sem::{AlterTable, PgBuiltInType},
        test_helpers::{alter_column_type, public_analysis},
    };

    #[rstest::rstest]
    #[case::assignment("ALTER TABLE t ALTER COLUMN a TYPE bigint", true, None)]
    #[case::using_other_column("ALTER TABLE t ALTER COLUMN a TYPE bigint USING b", true, None)]
    #[case::using_null("ALTER TABLE t ALTER COLUMN a TYPE bigint USING NULL", true, None)]
    #[case::using_unknown("ALTER TABLE t ALTER COLUMN a TYPE boolean USING 'true'", true, None)]
    #[case::invalid_reference(
        "ALTER TABLE t ALTER COLUMN a TYPE bigint USING absent",
        false,
        Some("alter_column_using_expression")
    )]
    #[case::parameter(
        "ALTER TABLE t ALTER COLUMN a TYPE bigint USING $1",
        false,
        Some("alter_column_using_expression")
    )]
    #[case::subquery(
        "ALTER TABLE t ALTER COLUMN a TYPE bigint USING (SELECT 1)",
        false,
        Some("alter_column_using_expression")
    )]
    #[case::invalid_modifier(
        "ALTER TABLE t ALTER COLUMN a TYPE numeric(0) USING NULL",
        false,
        Some("alter_column_invalid_typmod")
    )]
    #[case::missing_collation(
        "ALTER TABLE t ALTER COLUMN a TYPE integer COLLATE absent",
        false,
        Some("collation_not_found")
    )]
    #[case::missing_relation(
        "ALTER TABLE absent ALTER COLUMN a TYPE bigint",
        false,
        Some("alter_column_missing_relation")
    )]
    #[case::explicit_collation(
        "ALTER TABLE t ALTER COLUMN a TYPE text COLLATE \"C\" USING NULL",
        true,
        None
    )]
    #[case::noncollatable(
        "ALTER TABLE t ALTER COLUMN a TYPE integer COLLATE \"C\"",
        false,
        Some("collate_operand_type")
    )]
    #[case::missing_column(
        "ALTER TABLE t ALTER COLUMN missing TYPE bigint",
        false,
        Some("alter_column_missing_column")
    )]
    #[case::incompatible(
        "ALTER TABLE t ALTER COLUMN a TYPE boolean",
        false,
        Some("alter_column_type_conversion")
    )]
    #[case::missing_type(
        "ALTER TABLE t ALTER COLUMN a TYPE missing",
        false,
        Some("column_type_not_found")
    )]
    #[case::duplicate(
        "ALTER TABLE t ALTER COLUMN a TYPE bigint, ALTER COLUMN a TYPE bigint",
        false,
        Some("alter_column_duplicate_type")
    )]
    fn retains_conversion_and_failure_outcomes(
        #[case] sql: &str,
        #[case] applicable: bool,
        #[case] code: Option<&str>,
    ) -> testresult::TestResult {
        let statement = public_analysis::statement(sql, Dialect::postgres());
        let pair = AlterTable::analyze(&statement, alter_column_type::context())?;
        let analysis = pair.ast().alter_table_statement().unwrap();
        assert_eq!(analysis.can_apply(), applicable);
        assert!(analysis.requires_application_validation());
        let change = analysis.commands()[0].column_type_change().unwrap();
        assert_eq!(
            change.column(),
            &ColumnName::from(if sql.contains("missing TYPE") {
                "missing"
            } else {
                "a"
            })
        );
        if let Some(code) = code {
            assert!(
                pair.context().has_reported_problem(code),
                "{:?}",
                pair.context().reported_problems()
            );
        }
        let table = pair
            .context()
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        assert_eq!(
            table
                .get_column_def(&ColumnName::from("a"))
                .unwrap()
                .get_type(),
            Some(&PgBuiltInType::int4())
        );
        assert!(table.alter_statements().is_empty());
        Ok(())
    }

    #[test]
    fn absent_if_exists_skips_operand_and_type_resolution() -> testresult::TestResult {
        let statement = public_analysis::statement(
            "ALTER TABLE IF EXISTS absent ALTER COLUMN a TYPE missing USING unknown_column",
            Dialect::postgres(),
        );
        let result = AlterTable::analyze(&statement, alter_column_type::context())?;
        let alter = result.ast().alter_table_statement().unwrap();
        assert!(
            alter.commands()[0]
                .column_type_change()
                .unwrap()
                .is_skipped()
        );
        assert!(!alter.can_apply());
        assert!(result.context().reported_problems().is_empty());
        Ok(())
    }

    #[rstest::rstest]
    #[case::independent_default("ALTER TABLE t ALTER COLUMN a TYPE boolean USING true", false)]
    #[case::assignment_default("ALTER TABLE t ALTER COLUMN a TYPE bigint USING b", true)]
    fn default_is_checked_separately_from_using(
        #[case] sql: &str,
        #[case] valid: bool,
    ) -> testresult::TestResult {
        let statement = public_analysis::statement(sql, Dialect::postgres());
        let result = AlterTable::analyze(&statement, alter_column_type::with_default())?;
        let change = result.ast().alter_table_statement().unwrap().commands()[0]
            .column_type_change()
            .unwrap();
        assert!(change.value().can_assign());
        assert_eq!(change.default().unwrap().can_assign(), valid);
        assert_eq!(change.is_valid(), valid);
        assert_eq!(
            result
                .context()
                .has_reported_problem("alter_column_default_conversion"),
            !valid
        );
        Ok(())
    }
}

struct UsingExpression<'a, C> {
    context: &'a C,
    valid: bool,
}
impl<C: ParseContext> SemanticVisitor for UsingExpression<'_, C> {
    fn walk_source_column_values(&self) -> bool {
        false
    }
    fn enter_query(&mut self) {
        self.valid = false;
    }
    fn expression(&mut self, expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        self.valid &= expression.check_references_resolved()
            && expression.preserves_view_rows(self.context)
            && !matches!(expression, SemScalarExpr::Param(_));
        if let Some(collate) = expression.as_collate() {
            self.valid &= collate.can_apply(self.context);
        }
        Ok(())
    }
}

#[cfg(test)]
mod using_expression_tests {
    use crate::{
        ast::Dialect,
        sem::AlterTable,
        test_helpers::{alter_column_type, public_analysis},
    };

    #[rstest::rstest]
    #[case::noncollatable("a COLLATE \"C\"", false)]
    #[case::missing("'1' COLLATE absent", false)]
    #[case::nested("(a COLLATE \"C\")::bigint", false)]
    #[case::unknown_literal("'1' COLLATE \"C\"", true)]
    fn collate_outcomes_determine_using_validity(
        #[case] operand: &str,
        #[case] valid: bool,
    ) -> testresult::TestResult {
        let statement = public_analysis::statement(
            &format!("ALTER TABLE t ALTER COLUMN a TYPE bigint USING {operand}"),
            Dialect::postgres(),
        );
        let result = AlterTable::analyze(&statement, alter_column_type::context())?;
        let alter = result.ast().alter_table_statement().unwrap();
        let change = alter.commands()[0].column_type_change().unwrap();
        assert!(change.value().can_assign());
        assert_eq!(change.is_valid(), valid);
        assert_eq!(alter.can_apply(), valid);
        assert_eq!(
            result
                .context()
                .has_reported_problem("alter_column_using_expression"),
            !valid
        );
        Ok(())
    }
}

impl AlterColumnType {
    pub(crate) fn is_ast_relation(ast: &AstAlterRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| Self::ast_definition(command).is_some())
    }
    fn ordered_commands(ast: &AstAlterRelation) -> Vec<&AstAlterCommand> {
        // Attribute removals precede type analysis; additions and miscellaneous
        // attributes follow it. USING always binds against the original row.
        let mut ordered: Vec<_> = ast.commands().iter().collect();
        ordered.sort_by_key(|command| {
            if Self::ast_definition(command).is_some() {
                1
            } else {
                match ColumnAttributeAction::phase_from_ast_command(command) {
                    Some(0) => 0,
                    Some(1) => 2,
                    Some(_) => 3,
                    None => 4,
                }
            }
        });
        ordered
    }
    fn ast_definition(command: &AstAlterCommand) -> Option<(&str, &AstAlterColumnType)> {
        if let AstAlterCommand::Postgres(command) = command
            && let AstAlterDefinition::ColumnType(definition) = command.definition()
        {
            return Some((command.name(), definition));
        }
        None
    }
    pub(crate) fn analyze_relation(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let source = source.ok_or_else(|| {
            AnalysisError::unexpected_input_error("ALTER COLUMN source association")
        })?;
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested);
        let table = target.as_ref().unwrap_or(&requested);
        let definition = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let skipped = ast.if_exists()
            && context
                .resolve_relation_name(requested.full_name())
                .is_none();
        let mut names = Vec::new();
        let mut commands = Vec::new();
        let mut staged = definition.clone();
        let ordered = Self::ordered_commands(ast);
        for command in ordered {
            if let Some((name, syntax)) = Self::ast_definition(command) {
                let column = ColumnName::from(name);
                let duplicate = names.contains(&column);
                names.push(column.clone());
                let change = Self::from_ast(
                    syntax,
                    context,
                    table,
                    definition.as_ref(),
                    &column,
                    duplicate,
                    skipped,
                )?;
                commands.push(AlterObjSubCommand::column_type(&change));
            } else if ColumnAttributeAction::phase_from_ast_command(command).is_some() {
                commands.push(AlterObjSubCommand::from_ast_column_attribute(
                    command,
                    context,
                    table,
                    &mut staged,
                    skipped,
                )?);
            } else {
                todo!("type changes mixed with other unimplemented ALTER operations");
            }
        }
        Ok(SemAst::alter_table(&AlterTable::analyzed_column_types(
            table,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}

#[cfg(test)]
mod relation_tests {
    use crate::{
        ast::Dialect,
        sem::{AlterTable, PgBuiltInType},
        test_helpers::{alter_column_type, public_analysis},
    };

    #[test]
    fn multiple_changes_bind_using_to_the_original_row() -> testresult::TestResult {
        let statement = public_analysis::statement(
            "ALTER TABLE t ALTER COLUMN a TYPE boolean USING true, ALTER COLUMN b TYPE bigint USING a",
            Dialect::postgres(),
        );
        let result = AlterTable::analyze(&statement, alter_column_type::context())?;
        let alter = result.ast().alter_table_statement().unwrap();
        assert!(alter.can_apply());
        let second = alter.commands()[1].column_type_change().unwrap();
        assert_eq!(second.value().source(), Some(&PgBuiltInType::int4()));
        assert_eq!(second.original().unwrap().is_not_null(), Some(true));
        assert_eq!(second.value().target(), Some(&PgBuiltInType::int8()));
        Ok(())
    }

    #[test]
    fn mixed_attributes_keep_their_analyzed_outcomes() -> testresult::TestResult {
        let statement = public_analysis::statement(
            "ALTER TABLE t ALTER COLUMN a SET NOT NULL, ALTER COLUMN a TYPE bigint USING b::bigint, ALTER COLUMN a DROP NOT NULL",
            Dialect::postgres(),
        );
        let result = AlterTable::analyze(&statement, alter_column_type::context())?;
        let alter = result.ast().alter_table_statement().unwrap();
        assert!(alter.can_apply());
        assert_eq!(alter.commands().len(), 3);
        assert!(alter.commands()[0].column_attribute_change().is_some());
        let change = alter.commands()[1].column_type_change().unwrap();
        assert_eq!(change.value().source(), Some(&PgBuiltInType::int8()));
        assert_eq!(change.original().unwrap().is_not_null(), Some(false));
        assert!(alter.commands()[2].column_attribute_change().is_some());
        Ok(())
    }
}
