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

use crate::{
    sem::{
        AnalysisError, ColumnCollection, ParseContext, SemScalarExpr, TypeReference,
        create_comp_type::CompositeFieldDefinition,
        type_system::{
            ArgumentBindingCollection, CommonInferContext, SimpleInferContext, TypeError,
        },
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

pub trait FunctionParameterTrait: std::fmt::Display
where
    Self: Sized,
{
    fn name(&self) -> Option<&String>;
    fn param_type(&self) -> Option<&TypeReference>;
    fn default_expr(&self) -> Option<&SemScalarExpr>;
    fn is_input_param(&self) -> bool;

    fn from_syn<TParseContext>(
        context: TParseContext,
        param: crate::syn::FunctionParameter,
        tokens: &IVec<ScanToken>,
    ) -> Result<(FunctionParam, TParseContext), AnalysisError>
    where
        Self: Sized,
        TParseContext: ParseContext;

    fn bind_arg(
        &self,
        arg_types: &mut ArgumentBindingCollection,
    ) -> Result<ParameterBindResult, TypeError>;

    fn apply_simple_infer_result(
        &mut self,
        simple_infer_result: &crate::sem::type_system::SimpleInferResult,
    );
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
            Self::In(function_param_in) => write!(f, "{function_param_in}"),
            Self::Out(function_param_out) => write!(f, "{function_param_out}"),
            Self::InOut(function_param_in_out) => write!(f, "{function_param_in_out}"),
            Self::Variadic(function_param_variadic) => {
                write!(f, "{function_param_variadic}")
            }
            Self::Table(function_param_table) => write!(f, "{function_param_table}"),
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
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        match self {
            Self::In(value) => value.rename_relation(old, new),
            Self::Out(value) => value.rename_relation(old, new),
            Self::InOut(value) => value.rename_relation(old, new),
            Self::Variadic(value) => value.rename_relation(old, new),
            Self::Table(value) => value.rename_relation(old, new),
        }
    }

    pub(crate) const fn is_variadic(&self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    // PostgreSQL's variadic "any" accepts each argument independently. It is
    // neither a real cast target nor an array-polymorphic placeholder.
    pub(crate) fn accepts_any_type(&self) -> bool {
        self.is_variadic() && self.get_type() == Some(&TypeReference::from_master_type_name("any"))
    }

    pub(crate) fn call_argument_type(&self) -> Option<TypeReference> {
        let ty = self.get_type()?;
        if self.is_variadic() && ty.is_array() == Some(true) {
            Some(ty.element_type())
        } else {
            Some(ty.clone())
        }
    }

    pub(crate) fn identity_argument(&self) -> Option<Option<TypeReference>> {
        match self {
            Self::In(_) | Self::InOut(_) | Self::Variadic(_) => Some(self.get_type().cloned()),
            Self::Out(_) | Self::Table(_) => None,
        }
    }

    pub(crate) fn is_input(&self) -> bool {
        match self {
            Self::In(parameter) => parameter.is_input_param(),
            Self::InOut(parameter) => parameter.is_input_param(),
            Self::Variadic(parameter) => parameter.is_input_param(),
            Self::Out(parameter) => parameter.is_input_param(),
            Self::Table(parameter) => parameter.is_input_param(),
        }
    }

    pub(crate) fn new_in_out_param(
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default: &Option<SemScalarExpr>,
    ) -> Self {
        Self::InOut(FunctionParamInOut::new(
            name.as_ref(),
            param_type.as_ref(),
            default.as_ref(),
        ))
    }

    pub(crate) fn new_table_param(
        name: &Option<String>,
        param_type: &Option<TypeReference>,
    ) -> Self {
        Self::Table(FunctionParamTable::new(name.as_ref(), param_type.as_ref()))
    }
    /// get a type
    #[must_use]
    pub fn get_type(&self) -> Option<&TypeReference> {
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
            Self::InOut(p) => p.append_returning_column(column_collection),
            Self::Table(p) => p.append_returning_column(column_collection),
            _ => {}
        }
    }

    /// analyze output parameter as column collection
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] if any parameter fails to parse.
    pub fn collect_output_params_as_columns(
        params: &Vec<crate::syn::FunctionParameter>,
    ) -> Result<Option<ColumnCollection>, AnalysisError> {
        let mut columns = Vec::new();
        for param in params {
            if Self::is_output_param(param)? {
                let value = Self::analyze_as_column_definition(param)?;
                columns.push(value);
            }
        }
        Ok(Some(ColumnCollection::new(columns)))
    }

    /// bind argument to function parameter
    ///
    /// # Errors
    ///
    /// Propagates [`TypeError`] when binding fails.
    #[expect(rustdoc::private_intra_doc_links)]
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
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if inferring argument types fails.
    #[expect(rustdoc::private_intra_doc_links)]
    pub fn collect_simple(
        &self,
        context: &mut SimpleInferContext,
        catalog: &impl crate::context::BaseParseContext,
        bind: &ParameterBindResult,
    ) -> Result<(), TypeError> {
        match self {
            Self::In(p) => context.collect(p, bind, catalog),
            Self::Out(p) => context.collect(p, bind, catalog),
            Self::InOut(p) => context.collect(p, bind, catalog),
            Self::Variadic(p) => context.collect(p, bind, catalog),
            Self::Table(p) => context.collect(p, bind, catalog),
        }
    }

    /// collect common infer parameters
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if type inference fails.
    #[expect(rustdoc::private_intra_doc_links)]
    pub fn collect_common(
        &self,
        context: &mut CommonInferContext,
        catalog: &impl crate::context::BaseParseContext,
        bind: &ParameterBindResult,
    ) -> Result<(), TypeError> {
        match self {
            Self::In(p) => context.collect(p, bind, catalog),
            Self::Out(p) => context.collect(p, bind, catalog),
            Self::InOut(p) => context.collect(p, bind, catalog),
            Self::Variadic(p) => context.collect(p, bind, catalog),
            Self::Table(p) => context.collect(p, bind, catalog),
        }
    }

    /// apply simple infer result
    pub fn apply_simple_infer_result(
        &mut self,
        simple_infer_result: &crate::sem::type_system::SimpleInferResult,
    ) {
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

    /// tests implicit cast is available
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] when an implicit cast cannot be resolved.
    #[expect(rustdoc::private_intra_doc_links)]
    pub fn test_castability_one<TParseContext>(
        &self,
        context: &TParseContext,
        source_type: &Option<TypeReference>,
    ) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        self.castability_one_from_catalog(context, source_type)
    }

    pub(crate) fn castability_one_from_catalog<TCatalog: crate::context::BaseParseContext>(
        &self,
        context: &TCatalog,
        source_type: &Option<TypeReference>,
    ) -> Result<i32, TypeError> {
        if let Some(source_type) = source_type
            && let Some(target_type) = self.get_type()
        {
            if let Some(cast) = source_type.implicit_match_context(target_type, context) {
                Ok(cast.overload_priority_score())
            } else {
                TypeError::implicit_cast_not_found(
                    &format!("{source_type:?}"),
                    &format!("{target_type:?}"),
                )?;
                TypeError::never()
            }
        } else {
            Ok(0)
        }
    }

    /// tests implicit cast is available for variadic arguments
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] when any argument cannot be implicitly cast.
    #[expect(rustdoc::private_intra_doc_links)]
    pub fn test_castability_variadic<TParseContext>(
        &self,
        context: &TParseContext,
        source_types: &IVec<Option<TypeReference>>,
    ) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        self.castability_variadic_from_catalog(context, source_types)
    }

    pub(crate) fn castability_variadic_from_catalog<TCatalog: crate::context::BaseParseContext>(
        &self,
        context: &TCatalog,
        source_types: &IVec<Option<TypeReference>>,
    ) -> Result<i32, TypeError> {
        if self.accepts_any_type() {
            return Ok(0);
        }
        if let Some(target_type) = self.get_type() {
            let mut scores: Vec<i32> = Vec::new();
            #[expect(clippy::manual_flatten)]
            for source_type in source_types {
                if let Some(st) = source_type {
                    if let Some(cast) = st.implicit_match_context(target_type, context) {
                        scores.push(cast.overload_priority_score());
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

#[cfg(test)]
mod test_variadic_any_castability {
    use crate::sem::{FunctionParam, PgBuiltInType, TypeReference};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn heterogeneous_arguments_need_no_catalog_cast_or_exact_credit() {
        let parameter = FunctionParam::new_variadig_param(
            &None,
            &Some(TypeReference::from_master_type_name("any")),
        );
        let arguments = vec![
            Some(PgBuiltInType::int4()),
            Some(PgBuiltInType::bool()),
            Some(PgBuiltInType::int4().array_type()),
            None,
        ]
        .into();
        assert_eq!(
            parameter
                .test_castability_variadic(&TestParseContext::default(), &arguments)
                .unwrap(),
            0
        );
    }
}
