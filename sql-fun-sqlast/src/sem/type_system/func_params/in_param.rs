use crate::{
    sem::{
        AnalysisError, FromClause, FullName, ParseContext, SemScalarExpr, TypeReference,
        WithClause, analyze_scaler_expr,
        type_system::{ArgumentBindingCollection, TypeError},
    },
    syn::Opt,
};

use super::{FunctionParam, FunctionParameterTrait, ParameterBindResult};

impl FunctionParam {
    /// create new input parameter instance
    pub fn new_input_param(
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default_expr: &Option<SemScalarExpr>,
    ) -> Self {
        Self::In(FunctionParamIn::new(name, param_type, default_expr))
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
    pub fn new(
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default_expr: &Option<SemScalarExpr>,
    ) -> Self {
        Self {
            name: name.clone(),
            param_type: param_type.clone(),
            default_expr: default_expr.clone(),
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
        mut context: TParseContext,
        param: crate::syn::FunctionParameter,
    ) -> Result<(FunctionParam, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let param_name = param.get_name();
        let Some(param_type) = param.get_arg_type().as_inner() else {
            AnalysisError::raise_unexpected_none("function_parameter.arg_type")?
        };

        let (default_expr, new_context) = if let Some(def_expr) = param.get_defexpr().as_inner() {
            let with_clause = WithClause::default();
            let from_clause = FromClause::default();
            let (ex, new_context) =
                analyze_scaler_expr(context, &with_clause, &from_clause, def_expr)?;
            (Some(ex), new_context)
        } else {
            (None, context)
        };
        context = new_context;

        let param_type = FullName::try_from(param_type)?;
        let param_type = context.get_type(&param_type);
        Ok((
            FunctionParam::from(&Self {
                name: Some(param_name),
                param_type: param_type.and_then(|v| v.type_reference()).cloned(),
                default_expr,
            }),
            context,
        ))
    }

    fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        let Some(arg) = arg_types.pop_front() else {
            Err(TypeError::BindFailed(Box::new(FunctionParam::In(
                self.clone(),
            ))))?
        };
        Ok(ParameterBindResult::one_in(self, &arg))
    }

    fn is_input_param(&self) -> bool {
        true
    }

    fn apply_simple_infer_result(&mut self, simple_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_simple_infer_result(simple_infer_result)
        }
    }

    fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference) {
        if let Some(ty) = self.param_type.as_mut() {
            ty.apply_common_infer_result(common_infer_result);
        }
    }
}
