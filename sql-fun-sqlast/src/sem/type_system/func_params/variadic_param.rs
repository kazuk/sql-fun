use crate::{names::TableName, sem::type_system::SimpleInferResult};

use crate::sem::{
    FunctionParam, SemScalarExpr, TypeReference,
    type_system::{ArgumentBindingCollection, TypeError},
};

use crate::sem::type_system::func_params::{FunctionParameterTrait, ParameterBindResult};

impl FunctionParam {
    /// create new variadic parameter
    #[must_use]
    pub fn new_variadig_param(name: &Option<String>, param_type: &Option<TypeReference>) -> Self {
        Self::Variadic(FunctionParamVariadic::new(
            name.as_ref(),
            param_type.as_ref(),
        ))
    }
}

#[cfg(test)]
mod test_function_param_new_variadic_param {
    use crate::sem::{FunctionParam, TypeReference};

    #[test]
    fn test_new_variadic_param() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let var_param = FunctionParam::new_variadig_param(&name, &param_type);
        assert!(matches!(var_param, FunctionParam::Variadic(_)))
    }
}

impl From<&FunctionParamVariadic> for FunctionParam {
    fn from(value: &FunctionParamVariadic) -> Self {
        Self::Variadic(value.clone())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionParamVariadic {
    name: Option<String>,
    param_type: Option<TypeReference>,
}
impl FunctionParamVariadic {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(ty) = &mut self.param_type {
            ty.rename_relation(old, new);
        }
    }

    pub fn new(name: Option<&String>, param_type: Option<&TypeReference>) -> Self {
        Self {
            name: name.cloned(),
            param_type: param_type.cloned(),
        }
    }
}

impl std::fmt::Display for FunctionParamVariadic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.name, &self.param_type) {
            (Some(name), Some(tp)) => write!(f, "variadic parameter {name}:{tp}"),
            (Some(name), None) => write!(f, "variadic parameter {name}:(no-type)"),
            (None, Some(tp)) => write!(f, "variadic parameter (no-name):{tp}"),
            (None, None) => write!(f, "variadic parameter (no-name):(no-type)"),
        }
    }
}

impl FunctionParameterTrait for FunctionParamVariadic {
    fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    fn param_type(&self) -> Option<&TypeReference> {
        self.param_type.as_ref()
    }

    fn default_expr(&self) -> Option<&SemScalarExpr> {
        None
    }

    fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        let mut args = Vec::new();
        while let Some(arg) = arg_types.pop_front() {
            args.push(arg.clone());
        }
        Ok(ParameterBindResult::variadic(self, &args.into()))
    }

    fn is_input_param(&self) -> bool {
        true
    }

    fn apply_simple_infer_result(&mut self, simple_infer_result: &SimpleInferResult) {
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
