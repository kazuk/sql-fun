use crate::ast::ddl::{AlterOperation, PostgresAlterCommand};
use crate::ast::{AlterCommand, AlterRelation, Expr, Literal};
use crate::names::FullName;
use crate::sem::RelationAlteration;

use crate::{
    ast::{SyntaxOption, SyntaxValue, ddl::AlterDefinition},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AlterColumnAttributes, AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem,
        ColumnAttributeAction, ColumnAttributeBinding, ColumnAttributeOption, CreateTable, Kind,
        SemAst, SourcePathWithSpan, problem::AlterColumnProblem,
        search_path::expand_relation_search_path,
    },
};

impl RelationAlteration {
    pub(crate) fn has_ast_column_attributes(ast: &AlterRelation) -> bool {
        ast.commands
            .iter()
            .any(|command| ColumnAttributeAction::phase_from_ast_command(command).is_some())
    }

    pub(crate) fn from_ast_column_attributes(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        // Other relation kinds retain their own dispatch boundary.
        if !ast.kind.supports_table_column_attributes() {
            todo!("column attributes on non-table relations");
        }
        if !ast
            .commands
            .iter()
            .all(|command| ColumnAttributeAction::phase_from_ast_command(command).is_some())
        {
            todo!("column attributes mixed with other ALTER operations");
        }
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER COLUMN source");
        };
        let requested = TableName::from(FullName::from_ast_name(&ast.name)?);
        let target = context.resolve_table_name(&requested);
        let mut staged = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let table = target.as_ref().unwrap_or(&requested);
        let mut ordered: Vec<_> = ast.commands.iter().collect();
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
        let skip_missing = ast.if_exists && existing_relation.is_none() && target.is_none();
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
    use crate::sem::RelationAlteration;

    use crate::{
        ast::StatementNode,
        names::{ColumnName, FullName, TableName},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
            source_span,
        },
    };

    use crate::{ParseContext, context::BaseParseContext};

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

impl ColumnAttributeAction {
    pub(crate) fn phase_from_ast_command(ast: &AlterCommand) -> Option<u8> {
        if let AlterCommand::Postgres(command) = ast {
            Self::phase_from_ast_operation(&command.operation)
        } else {
            None
        }
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_column_attribute(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<Self, AnalysisError> {
        let AlterCommand::Postgres(command) = ast else {
            unreachable!("checked attribute command");
        };
        Self::from_ast_postgres_column_attribute(command, context, table, staged, if_exists)
    }
}

impl ColumnAttributeAction {
    pub(crate) const fn ast_operation_drops_not_null(ast: &AlterOperation) -> bool {
        matches!(ast, AlterOperation::DropNotNull)
    }

    pub(crate) const fn ast_operation_drops_expression(ast: &AlterOperation) -> bool {
        matches!(ast, AlterOperation::DropExpression)
    }

    pub(crate) const fn ast_operation_sets_statistics(ast: &AlterOperation) -> bool {
        matches!(ast, AlterOperation::SetStatistics)
    }

    pub(crate) fn from_ast_operation(
        ast: &AlterOperation,
        definition: &AlterDefinition,
        missing_ok: bool,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AlterOperation::SetNotNull => Ok(Self::not_null(true)),
            AlterOperation::DropNotNull => Ok(Self::not_null(false)),
            AlterOperation::DropExpression => Ok(Self::drop_expression(missing_ok)),
            AlterOperation::SetStatistics => Ok(Self::statistics(
                Self::statistics_from_ast_definition(definition)?,
            )),
            AlterOperation::SetOptions => Ok(Self::options(
                &ColumnAttributeOption::from_ast_definition_list(definition)?,
                false,
            )),
            AlterOperation::ResetOptions => Ok(Self::options(
                &ColumnAttributeOption::from_ast_definition_list(definition)?,
                true,
            )),
            _ => unreachable!("checked attribute operation"),
        }
    }

    pub(crate) const fn phase_from_ast_operation(ast: &AlterOperation) -> Option<u8> {
        match ast {
            AlterOperation::DropNotNull | AlterOperation::DropExpression => Some(0),
            AlterOperation::SetNotNull => Some(1),
            AlterOperation::SetStatistics
            | AlterOperation::SetOptions
            | AlterOperation::ResetOptions => Some(2),
            _ => None,
        }
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_single_column_attribute(
        ast: &PostgresAlterCommand,
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
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<Self, AnalysisError> {
        let column = ColumnName::from(ast.name.as_str());
        let action = ColumnAttributeAction::from_ast_operation(
            &ast.operation,
            &ast.definition,
            ast.missing_ok,
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
            if ColumnAttributeAction::ast_operation_drops_not_null(&ast.operation) {
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
            if ColumnAttributeAction::ast_operation_drops_expression(&ast.operation) {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                } else if !definition.has_stored_generation() && !ast.missing_ok {
                    problems.push(AlterColumnProblem::not_stored_generated());
                }
            }
            if ColumnAttributeAction::ast_operation_sets_statistics(&ast.operation) {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                }
                ColumnAttributeAction::validate_ast_statistics(
                    &ast.definition,
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

impl ColumnAttributeAction {
    pub(crate) fn statistics_from_ast_definition(
        ast: &AlterDefinition,
    ) -> Result<Option<i32>, AnalysisError> {
        match ast {
            AlterDefinition::None => Ok(None),
            AlterDefinition::Expression(expr) => Self::statistics_integer_from_ast_expression(expr)
                .map(Some)
                .ok_or_else(|| AnalysisError::unexpected_input_error("statistics integer")),
            _ => AnalysisError::raise_unexpected_input("statistics definition"),
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn from_ast_definition_list(
        ast: &AlterDefinition,
    ) -> Result<Vec<Self>, AnalysisError> {
        if let AlterDefinition::Options(options) = ast {
            options.iter().map(Self::from_ast).collect()
        } else {
            AnalysisError::raise_unexpected_input("column attribute options")
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) fn validate_ast_statistics(
        ast: &AlterDefinition,
        context: &impl ParseContext,
        problems: &mut Vec<AlterColumnProblem>,
    ) -> Result<(), AnalysisError> {
        match Self::statistics_from_ast_definition(ast)? {
            Some(value) if value < -1 => {
                problems.push(AlterColumnProblem::statistics_too_low(value))
            }
            Some(value) if value > 10000 => {
                problems.push(AlterColumnProblem::statistics_reduced(value))
            }
            None => {
                let configured = context.get_args().postgres_version(context.metadata())?;
                let Ok(version) = configured
                    .to_string()
                    .parse::<::sql_fun_core::PostgresVersion>()
                else {
                    todo!("custom PostgreSQL version capabilities");
                };
                if version.major() < 17 {
                    problems.push(AlterColumnProblem::version());
                }
            }
            _ => (),
        }
        Ok(())
    }

    pub(crate) const fn statistics_integer_from_ast_expression(ast: &Expr) -> Option<i32> {
        if let Expr::Literal(value) = ast {
            Self::statistics_integer_from_ast_literal(value)
        } else {
            None
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn value_from_ast_expression(ast: &Expr) -> Result<String, AnalysisError> {
        if let Expr::Literal(value) = ast {
            Self::value_from_ast_literal(value)
        } else {
            AnalysisError::raise_unexpected_input("column option constant")
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) const fn statistics_integer_from_ast_literal(ast: &Literal) -> Option<i32> {
        if let Literal::Integer(value) = ast {
            Some(*value)
        } else {
            None
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn value_from_ast_literal(ast: &Literal) -> Result<String, AnalysisError> {
        match ast {
            Literal::Integer(value) => Ok(value.to_string()),
            Literal::String(value) | Literal::Numeric(value) | Literal::Keyword(value) => {
                Ok(value.clone())
            }
            Literal::Boolean(value) => Ok(value.to_string()),
            _ => AnalysisError::raise_unexpected_input("column option literal"),
        }
    }

    pub(crate) fn from_ast(ast: &SyntaxOption) -> Result<Self, AnalysisError> {
        let name = if let Some(namespace) = &ast.namespace {
            format!("{namespace}.{}", ast.name)
        } else {
            ast.name.clone()
        };
        let value = ast.value.as_ref().map(Self::value_from_ast).transpose()?;
        Ok(Self::new(&name, value.as_deref()))
    }

    pub(crate) fn value_from_ast(ast: &SyntaxValue) -> Result<String, AnalysisError> {
        match ast {
            SyntaxValue::Expression(value) => Self::value_from_ast_expression(value),
            SyntaxValue::Type(value) => Ok(FullName::from_ast_name(&value.name)?.to_string()),
        }
    }
}
