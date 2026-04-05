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
    pub fn composite_field_definition(&self) -> Option<CompositeFieldDefinition> {
        todo!()
    }

    pub fn append_returning_column(&self, column_collection: &mut ColumnCollection) {
        let col_name = self.name.as_ref().map(|v| ColumnName::from(v.as_str()));
        let col_def = ColumnDefinition::new(&col_name, self.param_type.as_ref(), None, &[]);
        column_collection.push(&col_def);
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
        let param_type = context.get_type(&param_type);
        Ok((
            FunctionParam::Table(Self {
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
        Ok(ParameterBindResult::no_input(&FunctionParam::from(self)))
    }

    fn is_input_param(&self) -> bool {
        false
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
