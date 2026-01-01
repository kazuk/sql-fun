mod analyze;
mod bind_result;
mod in_out_param;
mod in_param;
mod out_param;
mod table_param;
mod variadic_param;

pub use self::bind_result::ParameterBindResult;
use self::{
    in_out_param::FunctionParamInOut, in_param::FunctionParamIn, out_param::FunctionParamOut,
    table_param::FunctionParamTable, variadic_param::FunctionParamVariadic,
};
use sql_fun_core::IVec;

use crate::sem::{
    AnalysisError, ColumnCollection, ParseContext, SemScalarExpr, TypeReference,
    create_comp_type::CompositeFieldDefinition,
    type_system::{ArgumentBindingCollection, CommonInferContext, SimpleInferContext, TypeError},
};

pub trait FunctionParameterTrait: std::fmt::Display
where
    Self: Sized,
{
    fn name(&self) -> &Option<String>;
    fn param_type(&self) -> &Option<TypeReference>;
    fn default_expr(&self) -> Option<&SemScalarExpr>;
    fn is_input_param(&self) -> bool;

    fn from_syn<TParseContext>(
        context: TParseContext,
        param: crate::syn::FunctionParameter,
    ) -> Result<(FunctionParam, TParseContext), AnalysisError>
    where
        Self: Sized,
        TParseContext: ParseContext;

    fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError>;

    fn apply_simple_infer_result(&mut self, simple_infer_result: &TypeReference);
    fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference);
}

/// function parameter type
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FunctionParam {
    /// Input parameter
    In(FunctionParamIn),
    /// output parameter
    Out(FunctionParamOut),
    /// input output parameter
    InOut(FunctionParamInOut),
    /// variadic parameter
    Variadic(FunctionParamVariadic),
    /// table parameter
    Table(FunctionParamTable),
}

impl std::fmt::Display for FunctionParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionParam::In(function_param_in) => write!(f, "{function_param_in}"),
            FunctionParam::Out(function_param_out) => write!(f, "{function_param_out}"),
            FunctionParam::InOut(function_param_in_out) => write!(f, "{function_param_in_out}"),
            FunctionParam::Variadic(function_param_variadic) => {
                write!(f, "{function_param_variadic}")
            }
            FunctionParam::Table(function_param_table) => write!(f, "{function_param_table}"),
        }
    }
}

#[cfg(test)]
mod test_function_param_display {
    use crate::sem::FunctionParam;

    #[rstest::rstest]
    fn test_function_param_display() {
        assert_eq!(
            format!(
                "{}",
                FunctionParam::new_input_param(&Some("para".to_string()), &None, &None)
            ),
            "input parameter para:(no-type)"
        )
    }
}

impl FunctionParam {
    /// get a type
    #[must_use]
    pub fn get_type(&self) -> &Option<TypeReference> {
        match self {
            Self::In(v) => v.param_type(),
            Self::Out(v) => v.param_type(),
            Self::InOut(v) => v.param_type(),
            Self::Variadic(v) => v.param_type(),
            Self::Table(v) => v.param_type(),
        }
    }

    /// append returning column to column collecttion
    pub fn append_returning_column(&self, column_collection: &mut ColumnCollection) {
        match self {
            Self::Out(p) => p.append_returning_column(column_collection),
            Self::InOut(p) => p.append_returnig_colum(column_collection),
            Self::Table(p) => p.append_returning_column(column_collection),
            _ => {}
        }
    }

    /// analyze output parameter as column collection
    pub fn collect_output_params_as_columns(
        params: &Vec<crate::syn::FunctionParameter>,
    ) -> Result<Option<ColumnCollection>, AnalysisError> {
        let mut columns = Vec::new();
        for param in params {
            if FunctionParam::is_output_param(param)? {
                let value = FunctionParam::analyze_as_column_definition(param)?;
                columns.push(value);
            }
        }
        Ok(Some(ColumnCollection::new(columns)))
    }

    /// bind argument to function parameter
    pub fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError> {
        match self {
            Self::In(function_param_in) => function_param_in.bind_arg(arg_types),
            Self::Out(function_param_out) => function_param_out.bind_arg(arg_types),
            Self::InOut(function_param_in_out) => function_param_in_out.bind_arg(arg_types),
            Self::Variadic(function_param_variadic) => function_param_variadic.bind_arg(arg_types),
            Self::Table(function_param_table) => function_param_table.bind_arg(arg_types),
        }
    }

    /// collect simple infer parameters
    pub fn collect_simple(
        &self,
        context: &mut SimpleInferContext,
        bind: &ParameterBindResult,
    ) -> Result<(), TypeError> {
        match self {
            Self::In(p) => context.collect(p, bind),
            Self::Out(p) => context.collect(p, bind),
            Self::InOut(p) => context.collect(p, bind),
            Self::Variadic(p) => context.collect(p, bind),
            Self::Table(p) => context.collect(p, bind),
        }
    }

    /// collect common infer parameters
    pub fn collect_common(
        &self,
        context: &mut CommonInferContext,
        bind: &ParameterBindResult,
    ) -> Result<(), TypeError> {
        match self {
            Self::In(p) => context.collect(p, bind),
            Self::Out(p) => context.collect(p, bind),
            Self::InOut(p) => context.collect(p, bind),
            Self::Variadic(p) => context.collect(p, bind),
            Self::Table(p) => context.collect(p, bind),
        }
    }

    /// apply simple infer result
    pub fn apply_simple_infer_result(&mut self, simple_infer_result: &TypeReference) {
        match self {
            Self::In(p) => p.apply_simple_infer_result(simple_infer_result),
            Self::Out(p) => p.apply_simple_infer_result(simple_infer_result),
            Self::InOut(p) => p.apply_simple_infer_result(simple_infer_result),
            Self::Variadic(p) => p.apply_simple_infer_result(simple_infer_result),
            Self::Table(p) => p.apply_simple_infer_result(simple_infer_result),
        }
    }

    /// apply common infer result
    pub fn apply_common_infer_result(&mut self, common_infer_result: &TypeReference) {
        match self {
            Self::In(p) => p.apply_common_infer_result(common_infer_result),
            Self::Out(p) => p.apply_common_infer_result(common_infer_result),
            Self::InOut(p) => p.apply_common_infer_result(common_infer_result),
            Self::Variadic(p) => p.apply_common_infer_result(common_infer_result),
            Self::Table(p) => p.apply_common_infer_result(common_infer_result),
        }
    }

    /// get composite type field definition for this parameter
    #[must_use]
    pub fn field_definition(&self) -> Option<CompositeFieldDefinition> {
        match self {
            Self::Out(p) => p.composite_field_definition(),
            Self::InOut(p) => p.composite_field_definition(),
            Self::Table(p) => p.composite_field_definition(),
            _ => None,
        }
    }

    /// tests implicit cast is avaiable
    pub fn test_castability_one<TParseContext>(
        &self,
        context: &TParseContext,
        source_type: &Option<TypeReference>,
    ) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        if let Some(source_type) = source_type
            && let Some(target_type) = self.get_type()
        {
            if let Some(cast) = context.get_implicit_cast(source_type, target_type) {
                Ok(cast.context().overload_priority_score())
            } else {
                TypeError::implicit_cast_not_found(
                    &format!("{source_type}"),
                    &format!("{target_type}"),
                )?;
                TypeError::never()
            }
        } else {
            Ok(0)
        }
    }

    /// tests implicit cast is avaiable for variadic arguments
    pub fn test_castability_variadic<TParseContext>(
        &self,
        context: &TParseContext,
        source_types: &IVec<Option<TypeReference>>,
    ) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        if let Some(target_type) = self.get_type() {
            let mut scores: Vec<i32> = Vec::new();
            #[expect(clippy::manual_flatten)]
            for source_type in source_types {
                if let Some(st) = source_type {
                    if let Some(cast) = context.get_implicit_cast(st, target_type) {
                        scores.push(cast.context().overload_priority_score());
                    } else {
                        TypeError::implicit_cast_not_found(
                            &format!("{st}"),
                            &format!("{target_type}"),
                        )?;
                        TypeError::never()
                    }
                }
            }
            Ok(scores.into_iter().min().unwrap_or(0))
        } else {
            Ok(0)
        }
    }
}
