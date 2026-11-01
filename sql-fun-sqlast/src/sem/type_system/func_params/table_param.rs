use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, ColumnCollection, ColumnDefinition, ColumnName, FullName, FunctionParam,
        ParseContext, SemScalarExpr, TypeReference,
        create_comp_type::CompositeFieldDefinition,
        type_system::{ArgumentBindingCollection, TypeError, func_params::ParameterBindResult},
    },
    syn::{Opt, ScanToken},
};

use super::FunctionParameterTrait;

impl From<&FunctionParamTable> for FunctionParam {
    fn from(value: &FunctionParamTable) -> Self {
        Self::Table(value.clone())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionParamTable {
    name: Option<String>,
    param_type: Option<TypeReference>,
}

impl FunctionParamTable {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        if let Some(ty) = &mut self.param_type {
            ty.rename_relation(old, new);
        }
    }

    pub(crate) fn new(name: Option<&String>, param_type: Option<&TypeReference>) -> Self {
        Self {
            name: name.cloned(),
            param_type: param_type.cloned(),
        }
    }
    /// Returns a nullable output field when both its name and type are known.
    /// Returns `None` if either is absent; no name or type is synthesized.
    pub fn composite_field_definition(&self) -> Option<CompositeFieldDefinition> {
        super::output_field_definition(self.name.as_ref(), self.param_type.as_ref())
    }

    pub fn append_returning_column(&self, column_collection: &mut ColumnCollection) {
        let col_name = self.name.as_ref().map(|v| ColumnName::from(v.as_str()));
        let col_def = ColumnDefinition::new(&col_name, self.param_type.as_ref(), None, &[]);
        column_collection.push(&col_def);
    }
}

#[cfg(test)]
mod test_output_field_definition {
    use super::FunctionParamTable;
    use crate::sem::{CompositeFieldDefinition, FunctionParam, TypeReference};
    use crate::test_helpers::dynamic_types;

    #[rstest::rstest]
    #[case::named_scalar(true, true, false)]
    #[case::named_owned_array(true, true, true)]
    #[case::unnamed(false, true, false)]
    #[case::unresolved(true, false, false)]
    #[case::absent(false, false, false)]
    fn preserves_known_output(
        #[case] has_name: bool,
        #[case] has_type: bool,
        #[case] owned_array: bool,
    ) {
        let name = if has_name {
            Some("Mixed Name".to_owned())
        } else {
            None
        };
        let scalar = TypeReference::from_master_type_name("int4");
        let param_type = if !has_type {
            None
        } else if owned_array {
            Some(dynamic_types::record(&scalar).array_type())
        } else {
            Some(scalar)
        };
        let parameter = FunctionParamTable::new(name.as_ref(), param_type.as_ref());
        let expected = if has_name && has_type {
            Some(CompositeFieldDefinition::new(
                "Mixed Name",
                &param_type,
                &Some(false),
            ))
        } else {
            None
        };
        assert_eq!(parameter.composite_field_definition(), expected);
        assert_eq!(FunctionParam::from(&parameter).field_definition(), expected);
    }
}

impl std::fmt::Display for FunctionParamTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.name, &self.param_type) {
            (Some(name), Some(tp)) => write!(f, "table parameter {name}:{tp}"),
            (Some(name), None) => write!(f, "table parameter {name}:(no-type)"),
            (None, Some(tp)) => write!(f, "table parameter (no-name):{tp}"),
            (None, None) => write!(f, "table parameter (no-name):(no-type)"),
        }
    }
}

impl FunctionParameterTrait for FunctionParamTable {
    fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    fn param_type(&self) -> Option<&TypeReference> {
        self.param_type.as_ref()
    }

    fn default_expr(&self) -> Option<&SemScalarExpr> {
        None
    }

    fn from_syn<TParseContext>(
        context: TParseContext,
        param: crate::syn::FunctionParameter,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(FunctionParam, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let param_name = param.get_name();
        let Some(param_type) = param.get_arg_type().as_inner() else {
            AnalysisError::raise_unexpected_none("function_parameter.arg_type")?
        };
        let param_type = FullName::try_from(param_type)?;
        let param_type = if let Some(definition) = context.get_type(&param_type) {
            definition.type_reference().cloned()
        } else {
            None
        };
        Ok((
            FunctionParam::Table(Self {
                name: Some(param_name),
                param_type,
            }),
            context,
        ))
    }

    fn bind_arg(
        &self,
        _arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        Ok(ParameterBindResult::no_input(&FunctionParam::from(self)))
    }

    fn is_input_param(&self) -> bool {
        false
    }

    fn apply_simple_infer_result(
        &mut self,
        simple_infer_result: &crate::sem::type_system::SimpleInferResult,
    ) {
        if let Some(ty) = self.param_type.as_mut() {
            simple_infer_result.apply(ty);
        }
    }

    fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_common_infer_result(common_infer_result);
        }
    }
}
