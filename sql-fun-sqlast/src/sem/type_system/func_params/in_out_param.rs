use crate::{
    names::TableName,
    sem::{
        ColumnCollection,
        type_system::{SimpleInferResult, func_params::output_field_definition},
    },
    syn::FunctionParameter,
};
use sql_fun_core::IVec;

use crate::{
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, ColumnDefinition, FromClause, FunctionParam, ParseContext,
        ScalarAnalysisContext, SemScalarExpr, TypeReference, WithClause,
        create_comp_type::CompositeFieldDefinition,
        type_system::{ArgumentBindingCollection, TypeError},
    },
    syn::{Opt, ScanToken},
};

use crate::sem::type_system::func_params::{FunctionParameterTrait, ParameterBindResult};

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

    /// Returns a nullable output field when both its name and type are known.
    /// Returns `None` if either is absent; no name or type is synthesized.
    pub fn composite_field_definition(&self) -> Option<CompositeFieldDefinition> {
        output_field_definition(self.name.as_ref(), self.param_type.as_ref())
    }
}

#[cfg(test)]
mod test_output_field_definition {
    use crate::{
        sem::{
            CompositeFieldDefinition, FunctionParam, TypeReference,
            type_system::func_params::in_out_param::FunctionParamInOut,
        },
        test_helpers::dynamic_types,
    };

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
        let parameter = FunctionParamInOut::new(name.as_ref(), param_type.as_ref(), None);
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

impl FunctionParamInOut {
    pub fn append_returning_column(&self, column_collection: &mut ColumnCollection) {
        let col_name = self.name.as_ref().map(|v| ColumnName::from(v.as_str()));
        let col_def = ColumnDefinition::new(&col_name, self.param_type.as_ref(), None, &[]);
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
        self.default_expr.as_ref()
    }

    fn from_syn<TParseContext>(
        mut context: TParseContext,
        param: FunctionParameter,
        tokens: &IVec<ScanToken>,
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
            let (ex, new_context) = SemScalarExpr::analyze_scaler_expr(
                context,
                &ScalarAnalysisContext::new(&with_clause, &from_clause),
                def_expr,
                tokens,
            )?;
            (Some(ex), new_context)
        } else {
            (None, context)
        };
        context = new_context;

        let param_type = FullName::try_from(param_type)?;
        let param_type = if let Some(definition) = context.get_type(&param_type) {
            definition.type_reference().cloned()
        } else {
            None
        };
        Ok((
            FunctionParam::from(&Self {
                name: Some(param_name),
                param_type,
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
            if self.default_expr.is_some() {
                return Ok(ParameterBindResult::no_input(&FunctionParam::from(self)));
            }
            Err(TypeError::BindFailed(Box::new(FunctionParam::InOut(
                self.clone(),
            ))))?
        };
        Ok(ParameterBindResult::one_in_out(self, arg.as_ref()))
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
