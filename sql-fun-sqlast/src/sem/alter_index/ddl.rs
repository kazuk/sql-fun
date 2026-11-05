//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, Expr as AstExpr, Literal as AstLiteral, Name as AstName,
        SyntaxOption as AstSyntaxOption, SyntaxValue as AstSyntaxValue, TypeName as AstTypeName,
        ddl::{
            AlterDefinition as AstAlterDefinition, AlterOperation as AstAlterOperation,
            PostgresAlterCommand as AstPostgresAlterCommand,
        },
    },
    context::ParseContext,
    sem::{AnalysisError, IndexAlteration, IndexStorageParameter},
};

impl IndexAlteration {
    pub(crate) fn from_ast_command(
        ast: &AstAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstAlterCommand::Postgres(command) => Self::from_ast_postgres_command(command, context),
            _ => todo!("ALTER INDEX command"),
        }
    }

    pub(crate) fn from_ast_postgres_command(
        ast: &AstPostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_operation(&ast.operation, context, ast.number, &ast.definition)
    }

    pub(crate) fn from_ast_operation(
        ast: &AstAlterOperation,
        _context: &mut impl ParseContext,
        number: i32,
        definition: &AstAlterDefinition,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstAlterOperation::SetRelOptions => {
                Self::from_ast_storage_definition(definition, false)
            }
            AstAlterOperation::ResetRelOptions => {
                Self::from_ast_storage_definition(definition, true)
            }
            AstAlterOperation::SetStatistics => {
                Self::from_ast_statistics_definition(definition, number)
            }
            _ => todo!("ALTER INDEX operation"),
        }
    }

    pub(crate) fn from_ast_storage_definition(
        ast: &AstAlterDefinition,
        reset: bool,
    ) -> Result<Self, AnalysisError> {
        let parameters = IndexStorageParameter::from_ast_definition_list(ast)?;
        Ok(Self::storage(reset, &parameters))
    }

    pub(crate) fn from_ast_statistics_definition(
        ast: &AstAlterDefinition,
        column: i32,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::statistics(
            column,
            Self::statistics_from_ast_definition(ast)?,
        ))
    }
}

impl IndexStorageParameter {
    pub(crate) fn from_ast_definition_list(
        ast: &AstAlterDefinition,
    ) -> Result<Vec<Self>, AnalysisError> {
        let AstAlterDefinition::Options(options) = ast else {
            return AnalysisError::raise_unexpected_input("index storage options");
        };
        Self::from_ast_list(options)
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_definition(
        ast: &AstAlterDefinition,
    ) -> Result<i32, AnalysisError> {
        let AstAlterDefinition::Expression(expression) = ast else {
            return AnalysisError::raise_unexpected_input("index statistics target");
        };
        Self::statistics_from_ast_expr(expression)
    }
}

impl IndexStorageParameter {
    pub(crate) fn from_ast_list(options: &[AstSyntaxOption]) -> Result<Vec<Self>, AnalysisError> {
        let mut parameters = Vec::new();
        for option in options {
            parameters.push(Self::try_from(option)?);
        }
        Ok(parameters)
    }
}

impl TryFrom<&AstSyntaxOption> for IndexStorageParameter {
    type Error = AnalysisError;
    fn try_from(option: &AstSyntaxOption) -> Result<Self, Self::Error> {
        let value = option
            .value
            .as_ref()
            .map(Self::value_from_ast_syntaxvalue)
            .transpose()?;
        Ok(Self::new(
            option.namespace.as_deref(),
            &option.name,
            value.as_deref(),
        ))
    }
}

impl IndexStorageParameter {
    pub(crate) fn value_from_ast_syntaxvalue(
        ast: &AstSyntaxValue,
    ) -> Result<String, AnalysisError> {
        match ast {
            AstSyntaxValue::Expression(expression) => Self::value_from_ast_expr(expression),
            AstSyntaxValue::Type(word) => Ok(Self::word_from_ast_typename(word)),
        }
    }

    pub(crate) fn value_from_ast_expr(ast: &AstExpr) -> Result<String, AnalysisError> {
        match ast {
            AstExpr::Literal(literal) => Self::value_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_expr(ast: &AstExpr) -> Result<i32, AnalysisError> {
        match ast {
            AstExpr::Literal(literal) => Self::statistics_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl IndexStorageParameter {
    pub(crate) fn value_from_ast_literal(ast: &AstLiteral) -> Result<String, AnalysisError> {
        match ast {
            AstLiteral::Integer(value) => Ok(value.to_string()),
            AstLiteral::Boolean(value) => Ok(value.to_string()),
            AstLiteral::Numeric(value) | AstLiteral::String(value) | AstLiteral::Keyword(value) => {
                Ok(value.clone())
            }
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_literal(ast: &AstLiteral) -> Result<i32, AnalysisError> {
        match ast {
            AstLiteral::Integer(value) => Ok(*value),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl IndexStorageParameter {
    pub(crate) fn word_from_ast_typename(ast: &AstTypeName) -> String {
        let mut word = Self::word_from_ast_name(&ast.name);
        for _ in &ast.array_bounds {
            word.push_str("[]");
        }
        word
    }

    pub(crate) fn word_from_ast_name(ast: &AstName) -> String {
        ast.0.join(".")
    }
}
