use crate::{
    ast::{FunctionParameter as AstFunctionParameter, TryFromWithTokens},
    names::{ColumnName, FullName},
    sem::{AnalysisError, ColumnDefinition, FunctionParam, ParseContext, TypeReference},
    syn::{FunctionParameter, FunctionParameterMode, Opt, ScanToken},
};
use sql_fun_core::IVec;

impl FunctionParam {
    /// analyze syntax node
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] when the function parameter mode or type is missing or invalid.
    pub fn analyze<TParseContext>(
        context: TParseContext,
        param: FunctionParameter,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_report(
            &AstFunctionParameter::try_from_with_tokens(&param, tokens)?,
            context,
        )
    }
}

impl super::FunctionParam {
    /// analyze [`crate::syn::FunctionParameter`] as column definition
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] when the parameter is missing its type or uses an invalid identifier.
    pub fn analyze_as_column_definition(
        param: &FunctionParameter,
    ) -> Result<ColumnDefinition, AnalysisError> {
        let Some(arg_type) = param.get_arg_type().as_inner() else {
            AnalysisError::raise_unexpected_none("functionparameter.arg_type")?
        };
        let arg_type = FullName::try_from(arg_type)?;
        let value = ColumnDefinition::new(
            &Some(ColumnName::from(param.get_name())),
            Some(&TypeReference::from_full_name(arg_type)),
            None,
            &[],
        );
        Ok(value)
    }
}

#[cfg(test)]
mod test_function_param_analyze_as_column_definition {
    use testresult::TestResult;

    use crate::{
        names::{FullName, LocalName, SchemaName},
        sem::{FunctionParam, TypeDefinition},
        syn::FunctionParameterMode,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_as_column_definition(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let int4_name = FullName::new(
            &None,
            &Some(SchemaName::from("pg_catalog")),
            &LocalName::from("int4"),
        );
        let int4_type = TypeDefinition::builtin(&int4_name, None);
        test_context.set_get_type_impl_result(&int4_name, &int4_type);
        let builder = SynBuilder::new();
        let arg_type = builder.type_name(&["int4"]);

        let func_param = builder.function_parameter(
            "param",
            arg_type.into(),
            FunctionParameterMode::FuncParamOut,
        );
        let _column_def = FunctionParam::analyze_as_column_definition(&func_param)?;
        assert!(FunctionParam::is_output_param(&func_param)?);
        Ok(())
    }
}

impl super::FunctionParam {
    /// test parameter is a output param
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] when the parameter mode is missing.
    pub fn is_output_param(param: &FunctionParameter) -> Result<bool, AnalysisError> {
        let Some(mode) = param.get_mode().as_inner() else {
            AnalysisError::raise_unexpected_none("functionparameter.mode")?
        };
        Ok(matches!(mode, FunctionParameterMode::FuncParamOut))
    }
}
