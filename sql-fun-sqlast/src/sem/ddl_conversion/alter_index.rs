use crate::ast::ddl::{AlterOperation, Index, PostgresAlterCommand};
use crate::ast::{AlterCommand, Expr, Literal, Name, SyntaxValue, TypeName};
use crate::sem::{AstConversion, convert};
use crate::{
    ast::{SyntaxOption, ddl::AlterDefinition},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CreateIndex, IndexAlteration, IndexStorageChange,
        IndexStorageParameter,
    },
};

impl AstConversion<'_, AlterCommand> {
    pub(crate) fn resolve_index(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<IndexAlteration, AnalysisError> {
        match self.ast {
            AlterCommand::Postgres(command) => {
                convert::<PostgresAlterCommand>(command).resolve_index(context)
            }
            _ => todo!("ALTER INDEX command"),
        }
    }
}

impl AstConversion<'_, PostgresAlterCommand> {
    pub(crate) fn resolve_index(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<IndexAlteration, AnalysisError> {
        convert(&self.ast.operation).resolve_index(context, self.ast.number, &self.ast.definition)
    }
}

impl AstConversion<'_, AlterOperation> {
    pub(crate) fn resolve_index(
        self,
        _context: &mut impl ParseContext,
        number: i32,
        definition: &AlterDefinition,
    ) -> Result<IndexAlteration, AnalysisError> {
        match self.ast {
            AlterOperation::SetRelOptions => convert(definition).resolve_index_storage(false),
            AlterOperation::ResetRelOptions => convert(definition).resolve_index_storage(true),
            AlterOperation::SetStatistics => convert(definition).resolve_index_statistics(number),
            _ => todo!("ALTER INDEX operation"),
        }
    }
}

impl AstConversion<'_, AlterDefinition> {
    pub(crate) fn resolve_index_storage(
        &self,
        reset: bool,
    ) -> Result<IndexAlteration, AnalysisError> {
        let parameters = self.index_storage_parameters()?;
        Ok(IndexAlteration::storage(reset, &parameters))
    }
    pub(crate) fn resolve_index_statistics(
        &self,
        column: i32,
    ) -> Result<IndexAlteration, AnalysisError> {
        Ok(IndexAlteration::statistics(
            column,
            self.index_statistics_target()?,
        ))
    }

    pub(crate) fn index_storage_parameters(
        &self,
    ) -> Result<Vec<IndexStorageParameter>, AnalysisError> {
        let AlterDefinition::Options(options) = self.ast else {
            return AnalysisError::raise_unexpected_input("index storage options");
        };
        AstConversion::<SyntaxOption>::index_storage_parameters(options)
    }
    pub(crate) fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        let AlterDefinition::Expression(expression) = self.ast else {
            return AnalysisError::raise_unexpected_input("index statistics target");
        };
        convert(expression).index_statistics_target()
    }
}

impl AstConversion<'_, SyntaxOption> {
    pub(crate) fn index_storage_parameters(
        options: &[SyntaxOption],
    ) -> Result<Vec<IndexStorageParameter>, AnalysisError> {
        let mut parameters = Vec::new();
        for option in options {
            parameters.push(IndexStorageParameter::try_from(option)?);
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
            .map(|value| convert(value).index_option_value())
            .transpose()?;
        Ok(Self::new(
            option.namespace.as_deref(),
            &option.name,
            value.as_deref(),
        ))
    }
}

impl AstConversion<'_, SyntaxValue> {
    pub(crate) fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self.ast {
            SyntaxValue::Expression(expression) => convert(expression).index_option_value(),
            SyntaxValue::Type(word) => Ok(convert(word).index_option_word()),
        }
    }
}

impl AstConversion<'_, Expr> {
    pub(crate) fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self.ast {
            Expr::Literal(literal) => convert(literal).index_option_value(),
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
    pub(crate) fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        match self.ast {
            Expr::Literal(literal) => convert(literal).index_statistics_target(),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl AstConversion<'_, Literal> {
    pub(crate) fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self.ast {
            Literal::Integer(value) => Ok(value.to_string()),
            Literal::Boolean(value) => Ok(value.to_string()),
            Literal::Numeric(value) | Literal::String(value) | Literal::Keyword(value) => {
                Ok(value.clone())
            }
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
    pub(crate) fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        match self.ast {
            Literal::Integer(value) => Ok(*value),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}

impl AstConversion<'_, Index> {
    pub(crate) fn initialize_index_storage(
        &self,
        context: &mut impl ParseContext,
        index: &mut CreateIndex,
    ) -> Result<bool, AnalysisError> {
        index.set_access_method(&self.ast.access_method);
        if index.access_method() != "btree" {
            return Ok(true);
        }
        let parameters =
            AstConversion::<SyntaxOption>::index_storage_parameters(&self.ast.options)?;
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

impl AstConversion<'_, TypeName> {
    pub(crate) fn index_option_word(&self) -> String {
        let mut word = convert(&self.ast.name).index_option_word();
        for _ in &self.ast.array_bounds {
            word.push_str("[]");
        }
        word
    }
}

impl AstConversion<'_, Name> {
    pub(crate) fn index_option_word(&self) -> String {
        self.ast.0.join(".")
    }
}
