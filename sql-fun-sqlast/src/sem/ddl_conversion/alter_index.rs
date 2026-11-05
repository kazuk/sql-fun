use crate::ast::ddl::{AlterOperation, Index, PostgresAlterCommand};
use crate::ast::{AlterCommand, Expr, Literal, Name, SyntaxValue, TypeName};
use crate::{
    ast::{SyntaxOption, ddl::AlterDefinition},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CreateIndex, IndexAlteration, IndexStorageChange,
        IndexStorageParameter,
    },
};

impl IndexAlteration {
    pub(crate) fn from_ast_command(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AlterCommand::Postgres(command) => Self::from_ast_postgres_command(command, context),
            _ => todo!("ALTER INDEX command"),
        }
    }

    pub(crate) fn from_ast_postgres_command(
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_operation(&ast.operation, context, ast.number, &ast.definition)
    }

    pub(crate) fn from_ast_operation(
        ast: &AlterOperation,
        _context: &mut impl ParseContext,
        number: i32,
        definition: &AlterDefinition,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AlterOperation::SetRelOptions => Self::from_ast_storage_definition(definition, false),
            AlterOperation::ResetRelOptions => Self::from_ast_storage_definition(definition, true),
            AlterOperation::SetStatistics => {
                Self::from_ast_statistics_definition(definition, number)
            }
            _ => todo!("ALTER INDEX operation"),
        }
    }

    pub(crate) fn from_ast_storage_definition(
        ast: &AlterDefinition,
        reset: bool,
    ) -> Result<Self, AnalysisError> {
        let parameters = IndexStorageParameter::from_ast_definition_list(ast)?;
        Ok(Self::storage(reset, &parameters))
    }

    pub(crate) fn from_ast_statistics_definition(
        ast: &AlterDefinition,
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
        ast: &AlterDefinition,
    ) -> Result<Vec<Self>, AnalysisError> {
        let AlterDefinition::Options(options) = ast else {
            return AnalysisError::raise_unexpected_input("index storage options");
        };
        Self::from_ast_list(options)
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_definition(
        ast: &AlterDefinition,
    ) -> Result<i32, AnalysisError> {
        let AlterDefinition::Expression(expression) = ast else {
            return AnalysisError::raise_unexpected_input("index statistics target");
        };
        Self::statistics_from_ast_expr(expression)
    }
}

impl IndexStorageParameter {
    pub(crate) fn from_ast_list(options: &[SyntaxOption]) -> Result<Vec<Self>, AnalysisError> {
        let mut parameters = Vec::new();
        for option in options {
            parameters.push(Self::try_from(option)?);
        }
        Ok(parameters)
    }
}

impl TryFrom<&SyntaxOption> for IndexStorageParameter {
    type Error = AnalysisError;
    fn try_from(option: &SyntaxOption) -> Result<Self, Self::Error> {
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
    pub(crate) fn value_from_ast_syntaxvalue(ast: &SyntaxValue) -> Result<String, AnalysisError> {
        match ast {
            SyntaxValue::Expression(expression) => Self::value_from_ast_expr(expression),
            SyntaxValue::Type(word) => Ok(Self::word_from_ast_typename(word)),
        }
    }

    pub(crate) fn value_from_ast_expr(ast: &Expr) -> Result<String, AnalysisError> {
        match ast {
            Expr::Literal(literal) => Self::value_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_expr(ast: &Expr) -> Result<i32, AnalysisError> {
        match ast {
            Expr::Literal(literal) => Self::statistics_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl IndexStorageParameter {
    pub(crate) fn value_from_ast_literal(ast: &Literal) -> Result<String, AnalysisError> {
        match ast {
            Literal::Integer(value) => Ok(value.to_string()),
            Literal::Boolean(value) => Ok(value.to_string()),
            Literal::Numeric(value) | Literal::String(value) | Literal::Keyword(value) => {
                Ok(value.clone())
            }
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
}

impl IndexAlteration {
    pub(crate) fn statistics_from_ast_literal(ast: &Literal) -> Result<i32, AnalysisError> {
        match ast {
            Literal::Integer(value) => Ok(*value),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl CreateIndex {
    pub(crate) fn retain_ast_storage(
        ast: &Index,
        context: &mut impl ParseContext,
        index: &mut Self,
    ) -> Result<bool, AnalysisError> {
        index.set_access_method(&ast.access_method);
        if index.access_method() != "btree" {
            return Ok(true);
        }
        let parameters = IndexStorageParameter::from_ast_list(&ast.options)?;
        let problems = IndexStorageChange::new(false, &parameters).evaluate(index);
        let mut valid = true;
        for problem in problems {
            if !problem.is_warning() {
                valid = false;
            }
            context.report_analysis_problem(AnalysisProblem::index(&problem))?;
        }
        Ok(valid)
    }
}

impl IndexStorageParameter {
    pub(crate) fn word_from_ast_typename(ast: &TypeName) -> String {
        let mut word = Self::word_from_ast_name(&ast.name);
        for _ in &ast.array_bounds {
            word.push_str("[]");
        }
        word
    }

    pub(crate) fn word_from_ast_name(ast: &Name) -> String {
        ast.0.join(".")
    }
}
