//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation,
        AlterRelationKind as AstAlterRelationKind, Dialect as AstDialect,
    },
    context::ParseContext,
    names::{FullName, TableName, ViewName},
    sem::{
        AlterColumnDefinition, AlterColumnType, AlterIndex, AlterObjSubCommand, AlterSequence,
        AlterTable, AlterView, AnalysisError, AnalysisProblem, ColumnAddition,
        ColumnAttributeAction, ColumnDrop, ColumnRename, EngineFeatureValidation, IndexAlteration,
        Kind, ResolutionFailure, RowSecurityChange, SemAst, SourcePathWithSpan,
        problem::ViewProblem, search_path::expand_relation_search_path,
    },
};

/// Coordinates ALTER semantics and catalog application across relation kinds.
pub(crate) struct RelationAlteration;

impl RelationAlteration {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstAlterRelation,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let source = context.get_source_path_with_span().cloned();
        Self::from_ast_and_apply_with_source(ast, context, source.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &AstAlterRelation,
        context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        Self::from_ast_and_apply_with_source(ast, context, Some(source))
    }

    pub(crate) fn from_ast_and_apply_with_source<TContext: ParseContext>(
        ast: &AstAlterRelation,
        mut context: TContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::alter_relation(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let changes = AlterView::from_ast_defaults(ast, &mut context);
        let Some(changes) = ResolutionFailure::finish(changes, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        if let Some(change) = changes {
            let context = context.apply_alter_view(&change)?;
            return Ok(AstAndContextPair::new(SemAst::alter_view(&change), context));
        }
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, &mut context, source);
        let statement = context.finish_analysis(result)?;
        let context = statement.apply_relation_alter(context)?;
        Ok(AstAndContextPair::new(statement, context))
    }
}

#[cfg(test)]
mod test_analyze_alter_table_statement {
    use crate::{
        AstAndContextPair,
        sem::{RelationAlteration, SemAst, SourcePathWithSpan},
        test_helpers::{TestParseContext, alter_syntax, source_span, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_table_statement(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        test_context.setup_table("table1", &[], &source_span);

        let syntax = alter_syntax::default_statement()?;
        let AstAndContextPair(res, ctx) =
            RelationAlteration::from_ast_and_apply_at(&syntax, test_context, &source_span)?;
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands().len(), 1);
        assert_eq!(ctx.alter_tables_len(), 1);
        assert_eq!(ctx.reported_problem_count(), 1);
        Ok(())
    }
}

impl RelationAlteration {
    pub(crate) fn from_ast_inner(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        if RowSecurityChange::is_ast_relation(ast) {
            return RowSecurityChange::analyze_relation(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && ColumnDrop::is_ast_relation(ast) {
            return ColumnDrop::analyze_relation(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && ColumnAddition::is_ast_relation(ast) {
            return ColumnAddition::analyze_relation(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes()
            && AlterColumnDefinition::is_ast_relation(ast)
        {
            return AlterColumnDefinition::analyze_relation(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && AlterColumnType::is_ast_relation(ast) {
            return AlterColumnType::analyze_relation(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && Self::has_ast_column_rename(ast) {
            return Self::from_ast_column_rename(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && Self::has_ast_table_rename(ast) {
            return Self::from_ast_table_rename(ast, context, source);
        }
        if ast.kind().supports_table_column_attributes() && Self::has_ast_column_attributes(ast) {
            return Self::from_ast_column_attributes(ast, context, source);
        }
        Self::from_ast_kind(
            &ast.kind(),
            context,
            &FullName::from_ast_name(ast.name())?,
            ast.commands(),
            source,
            ast.if_exists(),
        )
    }

    pub(crate) fn from_ast_kind(
        ast: &AstAlterRelationKind,
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AstAlterCommand],
        source: Option<&SourcePathWithSpan>,
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        match ast {
            AstAlterRelationKind::Table => {
                Self::table_from_ast_commands(context, name, syntax_commands, source)
            }
            AstAlterRelationKind::View => {
                Self::view_from_ast_commands(context, name, syntax_commands, false)
            }
            AstAlterRelationKind::MaterializedView => {
                Self::view_from_ast_commands(context, name, syntax_commands, true)
            }
            AstAlterRelationKind::Sequence => {
                Self::sequence_from_ast_commands(context, name, syntax_commands)
            }
            AstAlterRelationKind::Index => {
                Self::index_from_ast_commands(context, name, syntax_commands, if_exists)
            }
            AstAlterRelationKind::ForeignTable | AstAlterRelationKind::CompositeType => todo!(),
        }
    }

    pub(crate) fn index_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AstAlterCommand],
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        let mut operations = Vec::new();
        for command in syntax_commands {
            operations.push(IndexAlteration::from_ast_command(command, context)?);
        }
        let statement = AlterIndex::resolve(context, name, if_exists, &operations)?;
        Ok(SemAst::alter_index(&statement))
    }

    pub(crate) fn table_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AstAlterCommand],
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        let table = TableName::from(name.clone());
        if context.get_table(&table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&table))?;
        }
        let mut commands = Vec::new();
        for command in syntax_commands {
            commands.push(AlterObjSubCommand::from_ast(command, context, &table)?);
        }
        let mut statement = AlterTable::new(&table, &commands, source);
        statement.validate_check_names(context)?;
        Ok(SemAst::alter_table(&statement))
    }

    pub(crate) fn view_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AstAlterCommand],
        materialized: bool,
    ) -> Result<SemAst, AnalysisError> {
        let view = ViewName::from_full_name(name);
        let valid = context
            .get_view(&view)
            .is_some_and(|definition| definition.is_materialized() == materialized);
        if !valid {
            context.report_analysis_problem(AnalysisProblem::view(
                &ViewProblem::invalid_target(name),
            ))?;
            return Ok(SemAst::unresolved_statement());
        }
        let commands = syntax_commands
            .iter()
            .map(|command| AlterObjSubCommand::from_ast_owner(command, context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_view(&AlterView::new(&view, &commands)))
    }

    pub(crate) fn sequence_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AstAlterCommand],
    ) -> Result<SemAst, AnalysisError> {
        if context.get_sequence(name).is_none() {
            context.report_analysis_problem(AnalysisProblem::sequence_not_found(name))?;
        }
        let commands = syntax_commands
            .iter()
            .map(|command| AlterObjSubCommand::from_ast_owner(command, context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_sequence(&AlterSequence::new(name, &commands)))
    }
}

impl RelationAlteration {
    pub(crate) fn has_ast_table_rename(ast: &AstAlterRelation) -> bool {
        ast.commands().iter().any(AlterTable::is_ast_table_rename)
    }

    pub(crate) fn from_ast_table_rename(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("table rename source");
        };
        let [command] = ast.commands().as_slice() else {
            return AnalysisError::raise_unexpected_input("table rename must stand alone");
        };
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested).map(|target| {
            if context.dialect() == AstDialect::sqlite() {
                TableName::from(target.full_name().sqlite_identity())
            } else {
                target
            }
        });
        if target.is_none() {
            let existing = context.resolve_relation_name(requested.full_name());
            if !ast.if_exists() || existing.is_some() {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            }
        }
        let original = target.as_ref().unwrap_or(&requested).full_name();
        let destination = AlterTable::rename_destination_from_ast(command, original)?;
        let destination = if context.dialect() == AstDialect::sqlite() {
            TableName::from(destination.full_name().sqlite_identity())
        } else {
            destination
        };
        let permitted = if target.is_some() {
            context.permits_relation_creation(
                destination.full_name(),
                Kind::table(),
                false,
                false,
            )?
        } else {
            false
        };
        Ok(SemAst::alter_table(&AlterTable::table_rename(
            &requested,
            target.as_ref(),
            &destination,
            permitted,
            source,
        )))
    }
}

#[cfg(test)]
mod test_analyze_sqlite_alter_table_reports_missing_relation {
    #[cfg(feature = "sqlite")]
    use crate::{
        AstAndContextPair,
        ast::Dialect,
        sem::{RelationAlteration, SemAst, SourcePathWithSpan},
        test_helpers::{
            TestParseContext,
            alter_table::{parse_sqlite_alter_table, sqlite_context},
            source_span,
        },
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_missing_relation(source_span: SourcePathWithSpan) {
        let alter_table = parse_sqlite_alter_table("alter table users rename to members");
        let context = TestParseContext::default().with_dialect(Dialect::sqlite());

        let AstAndContextPair(res, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands().len(), 1);
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_existing_relation(source_span: SourcePathWithSpan) {
        let alter_table = parse_sqlite_alter_table("alter table users rename to members");
        let mut context = sqlite_context();
        context.setup_table("users", &[], &source_span);
        context.setup_table("members", &[], &source_span);

        let AstAndContextPair(_, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }
}

#[cfg(test)]
mod test_table_rename_applicability {
    use crate::{
        ParseContext,
        ast::StatementNode,
        context::TableInfoRead,
        names::FullName,
        sem::RelationAlteration,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
            schema_layer, source_span,
        },
    };

    #[test]
    fn suppressed_diagnostics_cannot_authorize_a_conflicting_rename() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE before_name(id integer)")?
                .into_context();
        let mut context =
            apply_ownership_sql(context, "CREATE TABLE occupied(id integer)")?.into_context();
        let syntax = ownership_statement("ALTER TABLE before_name RENAME TO occupied")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE syntax");
        };
        context.begin_parameter_type_analysis();
        let ast = RelationAlteration::from_ast_table_rename(
            &relation,
            &mut context,
            Some(&source_span()),
        )?;
        assert_eq!(context.problems().len(), 1);
        // Diagnostic delivery is independent of the retained applicability decision.
        let context = schema_layer(std::path::Path::new("rename.sql"), &context.freeze());
        assert!(context.problems().is_empty());
        assert!(!ast.alter_table_statement().unwrap().can_apply());
        let context = ast.apply_relation_alter(context)?;
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "before_name"))
                .is_some()
        );
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "occupied"))
                .is_some()
        );
        Ok(())
    }
}

impl RelationAlteration {
    pub(crate) fn has_ast_column_rename(ast: &AstAlterRelation) -> bool {
        ast.commands().iter().any(ColumnRename::is_ast_command)
    }

    pub(crate) fn from_ast_column_rename(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("column rename source");
        };
        let [command] = ast.commands().as_slice() else {
            return AnalysisError::raise_unexpected_input("column rename must stand alone");
        };
        let (from, to) = ColumnRename::names_from_ast(command)?;
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let (target, rename) =
            ColumnRename::resolve(context, &requested, &from, &to, ast.if_exists())?;
        Ok(SemAst::alter_table(&AlterTable::column_rename(
            &requested,
            target.as_ref(),
            &rename,
            source,
        )))
    }
}

#[cfg(test)]
mod test_analyze_sqlite_alter_table_reports_missing_column {
    #[cfg(feature = "sqlite")]
    use crate::{
        AstAndContextPair,
        sem::{RelationAlteration, SourcePathWithSpan},
        test_helpers::{
            alter_table::{make_column, parse_sqlite_alter_table, sqlite_context},
            source_span,
        },
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_missing_column(source_span: SourcePathWithSpan) {
        let alter_table =
            parse_sqlite_alter_table("alter table users rename column missing to renamed");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_rename_target_column_exists(
        source_span: SourcePathWithSpan,
    ) {
        let alter_table = parse_sqlite_alter_table("alter table users rename column name to email");
        let mut context = sqlite_context();
        context.setup_table(
            "users",
            &[make_column("name"), make_column("email")],
            &source_span,
        );

        let AstAndContextPair(_, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }
}

#[cfg(test)]
mod test_column_rename_applicability {
    use crate::{
        ParseContext,
        ast::StatementNode,
        context::TableInfoRead,
        sem::RelationAlteration,
        test_helpers::{
            column_rename::{name, schema},
            ownership::ownership_statement,
            source_span,
        },
    };

    #[test]
    fn suppressed_problems_do_not_authorize_application() -> testresult::TestResult {
        let mut context = schema(&["CREATE TABLE t(a integer, b integer)"])?;
        let syntax = ownership_statement("ALTER TABLE t RENAME a TO b")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE");
        };
        context.begin_parameter_type_analysis();
        let statement = RelationAlteration::from_ast_column_rename(
            &relation,
            &mut context,
            Some(&source_span()),
        )?;
        assert_eq!(context.analysis_state_mut().take_problems().len(), 1);
        assert!(!statement.alter_table_statement().unwrap().can_apply());
        let context = statement.apply_relation_alter(context)?;
        assert_eq!(
            context
                .get_table_impl(&name("t"))
                .unwrap()
                .alter_statements()
                .len(),
            0
        );
        Ok(())
    }
}

impl RelationAlteration {
    pub(crate) fn has_ast_column_attributes(ast: &AstAlterRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| ColumnAttributeAction::phase_from_ast_command(command).is_some())
    }

    pub(crate) fn from_ast_column_attributes(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        // Other relation kinds retain their own dispatch boundary.
        if !ast.kind().supports_table_column_attributes() {
            todo!("column attributes on non-table relations");
        }
        if !ast
            .commands()
            .iter()
            .all(|command| ColumnAttributeAction::phase_from_ast_command(command).is_some())
        {
            todo!("column attributes mixed with other ALTER operations");
        }
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER COLUMN source");
        };
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested);
        let mut staged = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let table = target.as_ref().unwrap_or(&requested);
        let mut ordered: Vec<_> = ast.commands().iter().collect();
        // PostgreSQL AT_PASS_DROP, AT_PASS_COL_ATTRS, AT_PASS_MISC. Stable sort
        // preserves the supplied order within a pass, including repeated changes.
        ordered.sort_by_key(|command| ColumnAttributeAction::phase_from_ast_command(command));
        let existing_relation =
            expand_relation_search_path(context, requested.full_name(), |name| {
                context.object_kind(name)
            });
        if existing_relation == Some(Kind::materialized_view())
            || existing_relation == Some(Kind::index())
        {
            todo!("column attributes on materialized views and indexes");
        }
        let skip_missing = ast.if_exists() && existing_relation.is_none() && target.is_none();
        let mut commands = Vec::new();
        for command in ordered {
            commands.push(AlterObjSubCommand::from_ast_column_attribute(
                command,
                context,
                table,
                &mut staged,
                skip_missing,
            )?);
        }
        Ok(SemAst::alter_table(&AlterTable::column_attributes(
            table,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}

#[cfg(test)]
mod test_applicability {
    use crate::{
        ParseContext,
        ast::StatementNode,
        context::BaseParseContext,
        names::{ColumnName, FullName, TableName},
        sem::RelationAlteration,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
            source_span,
        },
    };

    #[test]
    fn discarded_diagnostics_cannot_make_rejected_ddl_applicable() -> testresult::TestResult {
        let context = ownership_context();
        let context = apply_ownership_sql(context, "CREATE TABLE t(a integer)")?.into_context();
        let syntax =
            ownership_statement("ALTER TABLE t ALTER a SET NOT NULL, ALTER a SET STATISTICS -2")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE syntax");
        };
        let mut context = context;
        context.begin_parameter_type_analysis();
        let statement = RelationAlteration::from_ast_column_attributes(
            &relation,
            &mut context,
            Some(&source_span()),
        )?;
        // Simulate a diagnostic policy that suppresses delivery after analysis.
        let problems = context.analysis_state_mut().take_problems();
        assert_eq!(problems.len(), 1);
        let problem = problems[0]
            .alter_column_problem()
            .expect("structured diagnostic");
        assert_eq!(problem.column().as_str(), "a");
        assert_eq!(problem.table().as_str(), "public.t");
        assert!(problem.reason().is_error());
        assert!(context.problems().is_empty());
        assert!(!statement.alter_table_statement().unwrap().can_apply());
        let context = statement.apply_relation_alter(context)?;
        let table = context
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        let column = table.get_column_def(&ColumnName::from("a")).unwrap();
        assert_eq!(column.is_not_null(), Some(false));
        assert_eq!(column.statistics_target(), None);
        let result = apply_ownership_sql(context, "SELECT a FROM t")?;
        assert!(result.ast().select_statement().is_some());
        Ok(())
    }
}
