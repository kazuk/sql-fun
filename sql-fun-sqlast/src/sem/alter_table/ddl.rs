//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation, Column as AstColumn,
        Dialect as AstDialect, Expr as AstExpr, Name as AstName,
        TableConstraint as AstTableConstraint,
        ddl::{
            AlterOperation as AstAlterOperation, PostgresAlterCommand as AstPostgresAlterCommand,
        },
    },
    context::ParseContext,
    names::{ColumnName, FullName, LocalName, TableName, ViewName},
    sem::{
        AlterColumnAttributes, AlterObjSubCommand, AlterTable, AlterView, AnalysisError,
        AnalysisProblem, ColumnAddition, ColumnAttributeAction, ColumnAttributeBinding,
        ColumnDefinition, ColumnRename, Constraint, CreateTable, FromClause, OwnerSpecification,
        ResolutionFailure, ScalarAnalysisContext, SemScalarExpr, WithClause,
        problem::{AlterColumnProblem, ViewProblem},
        search_path::expand_relation_search_path,
    },
};

impl AlterObjSubCommand {
    #[cfg(test)]
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstAlterCommand,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context, table);
        let command = context.finish_analysis(result)?;
        Ok((command, context))
    }

    pub(crate) fn from_ast_owner(
        ast: &AstAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstAlterCommand::Postgres(command) => Self::from_ast_postgres_owner(command, context),
            AstAlterCommand::Owner(owner) => Ok(Self::resolved_owner(
                &OwnerSpecification::from_ast(owner, context)?,
            )),
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_change_owner {
    use crate::{
        sem::AlterObjSubCommand,
        test_helpers::{TestParseContext, alter_syntax, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_change_owner(mut test_context: TestParseContext) -> TestResult {
        let command = alter_syntax::owner()?;
        let _result = AlterObjSubCommand::from_ast_owner(&command, &mut test_context)?;
        Ok(())
    }
}

impl AlterObjSubCommand {
    pub(crate) fn check_ast_missing_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        if let Some(definition) = context.get_table(table)
            && !definition.has_column(column)
        {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                table, column,
            ))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_analyze_sqlite_alter_table_reports_drop_missing_column {
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
    fn test_analyze_sqlite_alter_table_reports_drop_missing_column(
        source_span: SourcePathWithSpan,
    ) {
        let alter_table = parse_sqlite_alter_table("alter table users drop column missing");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }
}

#[cfg(test)]
mod test_analyze_sqlite_alter_table_reports_existing_column {
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
    fn test_analyze_sqlite_alter_table_reports_existing_column(source_span: SourcePathWithSpan) {
        let alter_table = parse_sqlite_alter_table("alter table users add column name text");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context) =
            RelationAlteration::from_ast_and_apply_at(&alter_table, context, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast(
        ast: &AstAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstAlterCommand::Postgres(command) => Self::from_ast_postgres(command, context, table),
            AstAlterCommand::Owner(_) => Self::from_ast_owner(ast, context),
            AstAlterCommand::Default(name, expr) => {
                Self::from_ast_default(context, table, name, expr)
            }
            AstAlterCommand::Constraint(constraint) => {
                Self::from_ast_constraint(context, table, constraint)
            }
            AstAlterCommand::Cluster(name) => Self::from_ast_cluster(context, table, name),
            AstAlterCommand::Rename(name) => Self::from_ast_rename(context, name),
            AstAlterCommand::AddColumn(column) => Self::from_ast_add_column(context, table, column),
            AstAlterCommand::AlterColumn(name, column)
                if context.dialect() == AstDialect::sqlite() =>
            {
                Self::from_ast_alter_column(context, table, name, column)
            }
            AstAlterCommand::RenameColumn(from, to) => {
                Self::from_ast_rename_column(context, table, from, to)
            }
            AstAlterCommand::DropColumn(name) if context.dialect() == AstDialect::sqlite() => {
                Self::from_ast_drop_column(context, table, name)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn from_ast_default(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        expr: &AstExpr,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        let expr = SemScalarExpr::from_ast_in(expr, context, &ScalarAnalysisContext::default())?;
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::column_default(&name, &expr))
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_column_default {
    use crate::{
        names::{ColumnName, FullName, TableName},
        sem::{
            ColumnDefinition, ScalarConstExpr, SemScalarExpr, SourcePathWithSpan,
            alter_table::AlterObjSubCommand,
        },
        test_helpers::{TestParseContext, alter_syntax, source_span, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_column_default(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        // alter table ...
        //   alter column value set default(0)
        test_context.setup_table(
            "table1",
            &[ColumnDefinition::new(
                &Some(ColumnName::from("value")),
                None,
                None,
                &[],
            )],
            &source_span,
        );

        let command = alter_syntax::default_command()?;
        let table_name = TableName::from(FullName::with_schema("public", "table1"));
        let (res, test_context) =
            AlterObjSubCommand::from_ast_and_report(&command, test_context, &table_name)?;
        let AlterObjSubCommand::SetColumnDefault(col_name, expr) = res else {
            panic!("unexpected response {res:#?}");
        };
        assert_eq!(col_name.as_str(), "value");
        let SemScalarExpr::Const(ScalarConstExpr::Integer(val)) = *expr else {
            panic!("unexpected expr {expr:#?}");
        };
        assert_eq!(val, 0);
        assert_eq!(test_context.reported_problem_count(), 0);

        Ok(())
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_constraint(
        context: &mut impl ParseContext,
        table: &TableName,
        constraint: &AstTableConstraint,
    ) -> Result<Self, AnalysisError> {
        // Constraint-created indexes need the matched key even when the stored
        // table declaration remains unqualified.
        let resolved_table = context.resolve_table_name(table);
        let table = resolved_table.as_ref().unwrap_or(table);
        Ok(Self::add_constraint(&Constraint::from_ast_table(
            constraint, context, table,
        )?))
    }

    pub(crate) fn from_ast_cluster(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        let Some(resolved_table) = context.resolve_table_name(table) else {
            // The enclosing ALTER TABLE resolver already reports the absent table.
            return Ok(Self::cluster_on(name));
        };
        let index_name = FullName::new(
            &resolved_table.full_name().catalog().cloned(),
            &resolved_table.full_name().schema().cloned(),
            &LocalName::from(name),
        );
        if context.get_index(&index_name).is_none() {
            context.report_analysis_problem(AnalysisProblem::index_not_found(name))?;
        }
        Ok(Self::cluster_on(name))
    }

    pub(crate) fn from_ast_rename(
        context: &mut impl ParseContext,
        name: &AstName,
    ) -> Result<Self, AnalysisError> {
        let name = TableName::from(FullName::from_ast_name(name)?);
        if context.get_table(&name).is_some() {
            context.report_analysis_problem(AnalysisProblem::relation_already_exists(&name))?;
        }
        Ok(Self::rename_to(&name))
    }

    pub(crate) fn from_ast_add_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::add_column(&ColumnAddition::analyze_single(
            context, table, column,
        )?))
    }

    pub(crate) fn from_ast_alter_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        column: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::alter_column(
            &name,
            &ColumnDefinition::from_ast(column, context, table)?,
        ))
    }

    pub(crate) fn from_ast_rename_column(
        context: &mut impl ParseContext,
        table: &TableName,
        from: &str,
        to: &str,
    ) -> Result<Self, AnalysisError> {
        let (_, rename) = ColumnRename::resolve(
            context,
            table,
            &ColumnName::from(from),
            &ColumnName::from(to),
            false,
        )?;
        Ok(Self::analyzed_column_rename(&rename))
    }

    pub(crate) fn from_ast_drop_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::drop_column(&name))
    }

    pub(crate) fn from_ast_postgres_owner(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        if let Some(legacy) = &ast.legacy() {
            Self::from_ast_owner(legacy, context)
        } else {
            todo!()
        }
    }

    pub(crate) fn from_ast_postgres(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ColumnAttributeAction::phase_from_ast_operation(&ast.operation()).is_some() {
            return Self::from_ast_single_column_attribute(ast, context, table);
        }
        if let Some(legacy) = &ast.legacy() {
            Self::from_ast(legacy, context, table)
        } else {
            todo!()
        }
    }
}

impl AlterView {
    pub(crate) fn from_ast_defaults(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        let requested = FullName::from_ast_name(ast.name())?;
        let Some(name) = expand_relation_search_path(context, &requested, |key| {
            context
                .get_view_impl(key)
                .filter(|view| !view.is_materialized())
                .map(|_| key.clone())
        }) else {
            return Ok(None);
        };
        let mut actions = Vec::new();
        for command in ast.commands() {
            let Some(action) = AlterObjSubCommand::from_ast_view_default_action(command, context)?
            else {
                return Ok(None);
            };
            actions.push(action);
        }
        let change = Self::new(&ViewName::from_full_name(&name), &actions);
        let mut view = context
            .get_view_impl(&name)
            .expect("resolved ordinary View")
            .clone();
        if !view.apply_alter(context, &change)? {
            return Err(ResolutionFailure::reported());
        }
        Ok(Some(change))
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_view_default_action(
        ast: &AstAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            AstAlterCommand::Postgres(command) => {
                Self::from_ast_postgres_view_default_action(command, context)
            }
            AstAlterCommand::Default(name, expression) => {
                Self::from_ast_view_default(context, name, expression).map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn from_ast_postgres_view_default_action(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast.operation() {
            AstAlterOperation::ColumnDefault => {
                Self::from_ast_postgres_view_column_default(ast, context)
            }
            AstAlterOperation::EnableTrigger
            | AstAlterOperation::EnableAlwaysTrigger
            | AstAlterOperation::EnableTriggerAll
            | AstAlterOperation::EnableTriggerUser
            | AstAlterOperation::DisableTrigger
            | AstAlterOperation::EnableReplicaTrigger
            | AstAlterOperation::DisableTriggerAll
            | AstAlterOperation::DisableTriggerUser
            | AstAlterOperation::EnableRule
            | AstAlterOperation::EnableAlwaysRule
            | AstAlterOperation::DisableRule
            | AstAlterOperation::EnableReplicaRule => {
                Err(ResolutionFailure::view(&ViewProblem::invalid_alter()))
            }
            _ => Ok(None),
        }
    }

    fn from_ast_postgres_view_column_default(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        let value = SemScalarExpr::from_ast_view_default_definition(ast.definition(), context)?;
        let column = ColumnName::from(ast.name().as_str());
        let action = match value {
            Some(value) => Self::column_default(&column, &value),
            None => Self::drop_column_default(&column),
        };
        Ok(Some(action))
    }
}

impl AlterObjSubCommand {
    fn from_ast_view_default(
        context: &mut impl ParseContext,
        name: &str,
        expression: &AstExpr,
    ) -> Result<Self, ResolutionFailure> {
        let value = SemScalarExpr::from_ast(
            expression,
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )?;
        Ok(Self::column_default(&ColumnName::from(name), &value))
    }
}

impl AlterTable {
    pub(crate) const fn is_ast_table_rename(ast: &AstAlterCommand) -> bool {
        matches!(ast, AstAlterCommand::Rename(_))
    }

    pub(crate) fn rename_destination_from_ast(
        ast: &AstAlterCommand,
        original: &FullName,
    ) -> Result<TableName, AnalysisError> {
        let AstAlterCommand::Rename(name) = ast else {
            return AnalysisError::raise_unexpected_input("table rename command");
        };
        let local = FullName::from_ast_name(name)?;
        Ok(TableName::from(FullName::new(
            &original.catalog().cloned(),
            &original.schema().cloned(),
            local.name(),
        )))
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_column_attribute(
        ast: &AstAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<Self, AnalysisError> {
        let AstAlterCommand::Postgres(command) = ast else {
            unreachable!("checked attribute command");
        };
        Self::from_ast_postgres_column_attribute(command, context, table, staged, if_exists)
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_single_column_attribute(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        let resolved = context.resolve_table_name(table);
        let table = resolved.as_ref().unwrap_or(table);
        let mut staged = if let Some(name) = &resolved {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        Self::from_ast_postgres_column_attribute(ast, context, table, &mut staged, false)
    }

    pub(crate) fn from_ast_postgres_column_attribute(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<Self, AnalysisError> {
        let column = ColumnName::from(ast.name().as_str());
        let action = ColumnAttributeAction::from_ast_operation(
            &ast.operation(),
            ast.definition(),
            ast.missing_ok(),
        )?;
        let mut problems = Vec::new();
        let binding = match staged.as_ref() {
            None => {
                if !if_exists {
                    problems.push(AlterColumnProblem::missing_relation());
                }
                ColumnAttributeBinding::missing_relation()
            }
            Some(definition) => match definition.get_column_def(&column) {
                None => {
                    problems.push(AlterColumnProblem::missing_column());
                    ColumnAttributeBinding::missing_column()
                }
                Some(definition) => ColumnAttributeBinding::resolved(definition),
            },
        };
        if let Some(definition) = binding.definition() {
            if ColumnAttributeAction::ast_operation_drops_not_null(&ast.operation()) {
                if definition.has_inline_primary_key()
                    || staged
                        .as_ref()
                        .is_some_and(|table| table.primary_key_columns().contains(&&column))
                {
                    problems.push(AlterColumnProblem::primary_key());
                }
                if definition.has_identity() {
                    problems.push(AlterColumnProblem::identity());
                }
            }
            if ColumnAttributeAction::ast_operation_drops_expression(&ast.operation()) {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                } else if !definition.has_stored_generation() && !ast.missing_ok() {
                    problems.push(AlterColumnProblem::not_stored_generated());
                }
            }
            if ColumnAttributeAction::ast_operation_sets_statistics(&ast.operation()) {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                }
                ColumnAttributeAction::validate_ast_statistics(
                    ast.definition(),
                    context,
                    &mut problems,
                )?;
            }
            problems.extend(action.validate_options());
        }
        for problem in &problems {
            context
                .report_analysis_problem(AnalysisProblem::alter_column(table, &column, problem))?;
        }
        let change = AlterColumnAttributes::new(&column, &binding, &action, &problems);
        if let Some(staged) = staged {
            change.apply(staged);
        }
        Ok(Self::column_attributes(&change))
    }
}

#[cfg(test)]
mod test_standalone_semantic_command_api_resolves_column_attributes {
    use crate::{
        sem::AlterObjSubCommand,
        test_helpers::{alter_syntax, column_attributes as test_helpers},
    };

    #[test]
    fn standalone_semantic_command_api_resolves_column_attributes() -> testresult::TestResult {
        let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
        let syntax = alter_syntax::command("ALTER TABLE t ALTER a SET STATISTICS 42")?;
        let (command, context) =
            AlterObjSubCommand::from_ast_and_report(&syntax, context, &test_helpers::table_name())?;
        let change = command.column_attribute_change().unwrap();
        assert!(change.can_apply());
        assert_eq!(
            change.action().statistics_target().unwrap().effective(),
            Some(42)
        );
        assert!(change.binding().definition().is_some());
        assert!(context.problems().is_empty());
        // Command analysis returns semantics without applying the enclosing DDL.
        assert_eq!(
            test_helpers::column(&context, "a").statistics_target(),
            None
        );
        Ok(())
    }
}
