use crate::ast::ddl::{AlterDefinition, AlterOperation, Index, PostgresAlterCommand};
use crate::{
    ast::{AlterCommand, Expr, Literal, Name, SyntaxOption, SyntaxValue, TypeName},
    sem::{
        AnalysisError, AnalysisProblem, IndexAlteration, IndexStorageChange, IndexStorageParameter,
        ParseContext,
    },
};

impl AlterCommand {
    pub(super) fn resolve_index(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<IndexAlteration, AnalysisError> {
        match self {
            Self::Postgres(command) => command.resolve_index(context),
            _ => todo!("ALTER INDEX command"),
        }
    }
}
impl PostgresAlterCommand {
    fn resolve_index(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<IndexAlteration, AnalysisError> {
        self.operation
            .resolve_index(context, self.number, &self.definition)
    }
}
impl AlterOperation {
    fn resolve_index(
        self,
        _context: &mut impl ParseContext,
        number: i32,
        definition: &AlterDefinition,
    ) -> Result<IndexAlteration, AnalysisError> {
        match self {
            Self::SetRelOptions => definition.resolve_index_storage(false),
            Self::ResetRelOptions => definition.resolve_index_storage(true),
            Self::SetStatistics => definition.resolve_index_statistics(number),
            _ => todo!("ALTER INDEX operation"),
        }
    }
}
impl AlterDefinition {
    fn resolve_index_storage(&self, reset: bool) -> Result<IndexAlteration, AnalysisError> {
        let parameters = self.index_storage_parameters()?;
        Ok(IndexAlteration::storage(reset, &parameters))
    }
    fn resolve_index_statistics(&self, column: i32) -> Result<IndexAlteration, AnalysisError> {
        Ok(IndexAlteration::statistics(
            column,
            self.index_statistics_target()?,
        ))
    }

    fn index_storage_parameters(&self) -> Result<Vec<IndexStorageParameter>, AnalysisError> {
        let Self::Options(options) = self else {
            return AnalysisError::raise_unexpected_input("index storage options");
        };
        SyntaxOption::index_storage_parameters(options)
    }
    fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        let Self::Expression(expression) = self else {
            return AnalysisError::raise_unexpected_input("index statistics target");
        };
        expression.index_statistics_target()
    }
}
impl SyntaxOption {
    fn index_storage_parameters(
        options: &[Self],
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
            .map(SyntaxValue::index_option_value)
            .transpose()?;
        Ok(Self::new(
            option.namespace.as_deref(),
            &option.name,
            value.as_deref(),
        ))
    }
}
impl SyntaxValue {
    fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Expression(expression) => expression.index_option_value(),
            Self::Type(word) => Ok(word.index_option_word()),
        }
    }
}
impl Expr {
    fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Literal(literal) => literal.index_option_value(),
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
    fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        match self {
            Self::Literal(literal) => literal.index_statistics_target(),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}
impl Literal {
    fn index_option_value(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Integer(value) => Ok(value.to_string()),
            Self::Boolean(value) => Ok(value.to_string()),
            Self::Numeric(value) | Self::String(value) | Self::Keyword(value) => Ok(value.clone()),
            _ => AnalysisError::raise_unexpected_input("index option literal"),
        }
    }
    fn index_statistics_target(&self) -> Result<i32, AnalysisError> {
        match self {
            Self::Integer(value) => Ok(*value),
            _ => AnalysisError::raise_unexpected_input("index statistics integer"),
        }
    }
}
impl Index {
    pub(super) fn initialize_index_storage(
        &self,
        context: &mut impl ParseContext,
        index: &mut crate::sem::CreateIndex,
    ) -> Result<bool, AnalysisError> {
        index.set_access_method(&self.access_method);
        if index.access_method() != "btree" {
            return Ok(true);
        }
        let parameters = SyntaxOption::index_storage_parameters(&self.options)?;
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

// PostgreSQL represents unquoted reloption words (for example off) as a
// TypeName. Here they are parameter values, not references to database types.
impl TypeName {
    fn index_option_word(&self) -> String {
        let mut word = self.name.index_option_word();
        for _ in &self.array_bounds {
            word.push_str("[]");
        }
        word
    }
}
impl Name {
    fn index_option_word(&self) -> String {
        self.0.join(".")
    }
}
