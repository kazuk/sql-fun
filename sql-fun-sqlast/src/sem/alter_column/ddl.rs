//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, Expr as AstExpr, Literal as AstLiteral,
        SyntaxOption as AstSyntaxOption, SyntaxValue as AstSyntaxValue,
        ddl::{AlterDefinition as AstAlterDefinition, AlterOperation as AstAlterOperation},
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, ColumnAttributeAction, ColumnAttributeOption, problem::AlterColumnProblem,
    },
};

impl ColumnAttributeAction {
    pub(crate) fn phase_from_ast_command(ast: &AstAlterCommand) -> Option<u8> {
        if let AstAlterCommand::Postgres(command) = ast {
            Self::phase_from_ast_operation(&command.operation)
        } else {
            None
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) const fn ast_operation_drops_not_null(ast: &AstAlterOperation) -> bool {
        matches!(ast, AstAlterOperation::DropNotNull)
    }

    pub(crate) const fn ast_operation_drops_expression(ast: &AstAlterOperation) -> bool {
        matches!(ast, AstAlterOperation::DropExpression)
    }

    pub(crate) const fn ast_operation_sets_statistics(ast: &AstAlterOperation) -> bool {
        matches!(ast, AstAlterOperation::SetStatistics)
    }

    pub(crate) fn from_ast_operation(
        ast: &AstAlterOperation,
        definition: &AstAlterDefinition,
        missing_ok: bool,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstAlterOperation::SetNotNull => Ok(Self::not_null(true)),
            AstAlterOperation::DropNotNull => Ok(Self::not_null(false)),
            AstAlterOperation::DropExpression => Ok(Self::drop_expression(missing_ok)),
            AstAlterOperation::SetStatistics => Ok(Self::statistics(
                Self::statistics_from_ast_definition(definition)?,
            )),
            AstAlterOperation::SetOptions => Ok(Self::options(
                &ColumnAttributeOption::from_ast_definition_list(definition)?,
                false,
            )),
            AstAlterOperation::ResetOptions => Ok(Self::options(
                &ColumnAttributeOption::from_ast_definition_list(definition)?,
                true,
            )),
            _ => unreachable!("checked attribute operation"),
        }
    }

    pub(crate) const fn phase_from_ast_operation(ast: &AstAlterOperation) -> Option<u8> {
        match ast {
            AstAlterOperation::DropNotNull | AstAlterOperation::DropExpression => Some(0),
            AstAlterOperation::SetNotNull => Some(1),
            AstAlterOperation::SetStatistics
            | AstAlterOperation::SetOptions
            | AstAlterOperation::ResetOptions => Some(2),
            _ => None,
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) fn statistics_from_ast_definition(
        ast: &AstAlterDefinition,
    ) -> Result<Option<i32>, AnalysisError> {
        match ast {
            AstAlterDefinition::None => Ok(None),
            AstAlterDefinition::Expression(expr) => {
                Self::statistics_integer_from_ast_expression(expr)
                    .map(Some)
                    .ok_or_else(|| AnalysisError::unexpected_input_error("statistics integer"))
            }
            _ => AnalysisError::raise_unexpected_input("statistics definition"),
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn from_ast_definition_list(
        ast: &AstAlterDefinition,
    ) -> Result<Vec<Self>, AnalysisError> {
        if let AstAlterDefinition::Options(options) = ast {
            options.iter().map(Self::from_ast).collect()
        } else {
            AnalysisError::raise_unexpected_input("column attribute options")
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) fn validate_ast_statistics(
        ast: &AstAlterDefinition,
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

    pub(crate) const fn statistics_integer_from_ast_expression(ast: &AstExpr) -> Option<i32> {
        if let AstExpr::Literal(value) = ast {
            Self::statistics_integer_from_ast_literal(value)
        } else {
            None
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn value_from_ast_expression(ast: &AstExpr) -> Result<String, AnalysisError> {
        if let AstExpr::Literal(value) = ast {
            Self::value_from_ast_literal(value)
        } else {
            AnalysisError::raise_unexpected_input("column option constant")
        }
    }
}

impl ColumnAttributeAction {
    pub(crate) const fn statistics_integer_from_ast_literal(ast: &AstLiteral) -> Option<i32> {
        if let AstLiteral::Integer(value) = ast {
            Some(*value)
        } else {
            None
        }
    }
}

impl ColumnAttributeOption {
    pub(crate) fn value_from_ast_literal(ast: &AstLiteral) -> Result<String, AnalysisError> {
        match ast {
            AstLiteral::Integer(value) => Ok(value.to_string()),
            AstLiteral::String(value) | AstLiteral::Numeric(value) | AstLiteral::Keyword(value) => {
                Ok(value.clone())
            }
            AstLiteral::Boolean(value) => Ok(value.to_string()),
            _ => AnalysisError::raise_unexpected_input("column option literal"),
        }
    }

    pub(crate) fn from_ast(ast: &AstSyntaxOption) -> Result<Self, AnalysisError> {
        let name = if let Some(namespace) = &ast.namespace {
            format!("{namespace}.{}", ast.name)
        } else {
            ast.name.clone()
        };
        let value = ast.value.as_ref().map(Self::value_from_ast).transpose()?;
        Ok(Self::new(&name, value.as_deref()))
    }

    pub(crate) fn value_from_ast(ast: &AstSyntaxValue) -> Result<String, AnalysisError> {
        match ast {
            AstSyntaxValue::Expression(value) => Self::value_from_ast_expression(value),
            AstSyntaxValue::Type(value) => Ok(FullName::from_ast_name(&value.name)?.to_string()),
        }
    }
}
