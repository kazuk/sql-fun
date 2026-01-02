use crate::{
    sem::{
        AnalysisError, ColumnDefinition, ColumnName, FromClause, FullName, FunctionParam,
        ParseContext, SemScalarExpr, TypeReference, WithClause, analyze_scaler_expr,
        create_comp_type::CompositeFieldDefinition,
        type_system::{ArgumentBindingCollection, TypeError},
    },
    syn::Opt,
};

use super::FunctionParameterTrait;
use super::ParameterBindResult;

impl From<&FunctionParamInOut> for FunctionParam {
    fn from(value: &FunctionParamInOut) -> Self {
        Self::InOut(value.clone())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionParamInOut {
    name: Option<String>,
    param_type: Option<TypeReference>,
    default_expr: Option<SemScalarExpr>,
}

impl FunctionParamInOut {
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

    pub fn composite_field_definition(&self) -> Option<CompositeFieldDefinition> {
        todo!()
    }

    pub fn append_returnig_colum(&self, column_collection: &mut crate::sem::ColumnCollection) {
        let col_name = self.name.as_ref().map(|v| ColumnName::from(v.as_str()));
        let col_def = ColumnDefinition::new(&col_name, self.param_type.as_ref(), None);
        column_collection.push(&col_def);
    }
}

impl std::fmt::Display for FunctionParamInOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.name, &self.param_type) {
            (Some(name), Some(tp)) => write!(f, "input-output parameter {name}:{tp}"),
            (Some(name), None) => write!(f, "input-output parameter {name}:(no-type)"),
            (None, Some(tp)) => write!(f, "input-output parameter (no-name):{tp}"),
            (None, None) => write!(f, "input-output parameter (no-name):(no-type)"),
        }
    }
}

impl FunctionParameterTrait for FunctionParamInOut {
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
            Err(TypeError::BindFailed(Box::new(FunctionParam::InOut(
                self.clone(),
            ))))?
        };
        Ok(ParameterBindResult::one_in_out(self, arg.as_ref()))
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
