use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FullName, FunctionParam, ParseContext, SemScalarExpr, TypeReference,
        type_system::{ArgumentBindingCollection, TypeError},
    },
    syn::{Opt, ScanToken},
};

use super::{FunctionParameterTrait, ParameterBindResult};

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

    fn apply_simple_infer_result(&mut self, simple_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_simple_infer_result(simple_infer_result);
        }
    }

    fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_common_infer_result(common_infer_result);
        }
    }
}
