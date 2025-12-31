use crate::{
    sem::{
        AnalysisError, ColumnCollection, ColumnDefinition, ColumnName, FullName, ParseContext,
        SemScalarExpr, TypeReference,
        create_comp_type::CompositeFieldDefinition,
        type_system::{ArgumentBindingCollection, TypeError},
    },
    syn::Opt,
};

use super::{FunctionParam, FunctionParameterTrait, ParameterBindResult};

impl FunctionParam {
    /// create new output parameter instance
    pub fn new_output_param(name: &Option<String>, param_type: &Option<TypeReference>) -> Self {
        Self::Out(FunctionParamOut::new(name, param_type))
    }
}

#[cfg(test)]
mod test_function_param_new_output_param {
    use crate::sem::{FunctionParam, TypeReference};

    #[test]
    fn test_new_output_param() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let out_param = FunctionParam::new_output_param(&name, &param_type);
        assert!(matches!(out_param, FunctionParam::Out(_)))
    }
}

impl From<&FunctionParamOut> for FunctionParam {
    fn from(value: &FunctionParamOut) -> Self {
        Self::Out(value.clone())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionParamOut {
    name: Option<String>,
    param_type: Option<TypeReference>,
}

impl FunctionParamOut {
    pub fn new(name: &Option<String>, param_type: &Option<TypeReference>) -> Self {
        Self {
            name: name.clone(),
            param_type: param_type.clone(),
        }
    }

    pub fn composite_field_definition(&self) -> Option<CompositeFieldDefinition> {
        if let Some(name) = &self.name
            && let Some(type_ref) = &self.param_type
        {
            Some(CompositeFieldDefinition::new(
                name,
                &Some(type_ref.clone()),
                &Some(false),
            ))
        } else {
            None
        }
    }

    pub fn append_returning_column(&self, column_collection: &mut ColumnCollection) {
        let col_name = self.name.as_ref().map(|v| ColumnName::from(v.as_str()));
        let col_def = ColumnDefinition::new(&col_name, self.param_type.as_ref(), None);
        column_collection.push(&col_def);
    }
}

impl std::fmt::Display for FunctionParamOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.name, &self.param_type) {
            (Some(name), Some(tp)) => write!(f, "output parameter {name}:{tp}"),
            (Some(name), None) => write!(f, "output parameter {name}:(no-type)"),
            (None, Some(tp)) => write!(f, "output parameter (no-name):{tp}"),
            (None, None) => write!(f, "output parameter (no-name):(no-type)"),
        }
    }
}

impl FunctionParameterTrait for FunctionParamOut {
    fn name(&self) -> &Option<String> {
        &self.name
    }

    fn param_type(&self) -> &Option<TypeReference> {
        &self.param_type
    }

    fn default_expr(&self) -> Option<&SemScalarExpr> {
        None
    }

    fn from_syn<TParseContext>(
        context: TParseContext,
        param: crate::syn::FunctionParameter,
    ) -> Result<(FunctionParam, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let param_name = param.get_name();
        let Some(param_type) = param.get_arg_type().as_inner() else {
            AnalysisError::raise_unexpected_none("function_parameter.arg_type")?
        };
        let param_type = FullName::try_from(param_type)?;
        let param_type = context.get_type(&param_type);
        Ok((
            FunctionParam::from(&Self {
                name: Some(param_name),
                param_type: param_type.and_then(|v| v.type_reference()).cloned(),
            }),
            context,
        ))
    }

    fn bind_arg(
        &self,
        _arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        Ok(ParameterBindResult::NoInputParam(FunctionParam::Out(
            self.clone(),
        )))
    }

    fn is_input_param(&self) -> bool {
        false
    }

    fn apply_simple_infer_result(&mut self, simple_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_simple_infer_result(simple_infer_result)
        }
    }

    fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_common_infer_result(common_infer_result)
        }
    }
}
