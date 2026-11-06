use crate::{names::TableName, sem::type_system::SimpleInferResult};

use crate::sem::{
    SemScalarExpr, TypeReference,
    type_system::{ArgumentBindingCollection, TypeError},
};

use crate::sem::type_system::func_params::{
    FunctionParam, FunctionParameterTrait, ParameterBindResult,
};

impl FunctionParam {
    /// create new input parameter instance
    #[must_use]
    pub fn new_input_param(
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default_expr: &Option<SemScalarExpr>,
    ) -> Self {
        Self::In(FunctionParamIn::new(
            name.as_ref(),
            param_type.as_ref(),
            default_expr.as_ref(),
        ))
    }
}

#[cfg(test)]
mod test_function_param_new_input_param {
    use crate::sem::{FunctionParam, TypeReference};

    #[test]
    fn test_new_input_param() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let default_expr = None;
        let in_param = FunctionParam::new_input_param(&name, &param_type, &default_expr);
        assert!(matches!(in_param, FunctionParam::In(_)))
    }
}

impl From<&FunctionParamIn> for FunctionParam {
    fn from(value: &FunctionParamIn) -> Self {
        Self::In(value.clone())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionParamIn {
    name: Option<String>,
    param_type: Option<TypeReference>,
    default_expr: Option<SemScalarExpr>,
}

impl FunctionParamIn {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(ty) = &mut self.param_type {
            ty.rename_relation(old, new);
        }
    }

    pub fn new(
        name: Option<&String>,
        param_type: Option<&TypeReference>,
        default_expr: Option<&SemScalarExpr>,
    ) -> Self {
        Self {
            name: name.cloned(),
            param_type: param_type.cloned(),
            default_expr: default_expr.cloned(),
        }
    }
}

impl std::fmt::Display for FunctionParamIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.name, &self.param_type) {
            (Some(name), Some(tp)) => write!(f, "input parameter {name}:{tp}"),
            (Some(name), None) => write!(f, "input parameter {name}:(no-type)"),
            (None, Some(tp)) => write!(f, "input parameter (no-name):{tp}"),
            (None, None) => write!(f, "input parameter (no-name):(no-type)"),
        }
    }
}

impl FunctionParameterTrait for FunctionParamIn {
    fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    fn param_type(&self) -> Option<&TypeReference> {
        self.param_type.as_ref()
    }

    fn default_expr(&self) -> Option<&SemScalarExpr> {
        self.default_expr.as_ref()
    }

    fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        let Some(arg) = arg_types.pop_front() else {
            if self.default_expr.is_some() {
                return Ok(ParameterBindResult::no_input(&FunctionParam::from(self)));
            }
            Err(TypeError::BindFailed(Box::new(FunctionParam::In(
                self.clone(),
            ))))?
        };
        Ok(ParameterBindResult::one_in(self, arg.as_ref()))
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
