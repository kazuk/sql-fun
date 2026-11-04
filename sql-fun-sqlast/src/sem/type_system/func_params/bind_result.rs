use crate::{
    context::BaseParseContext,
    sem::{
        FunctionParam, ParseContext, TypeReference,
        overload_rank::OverloadRank,
        type_system::{
            CommonInferContext, SimpleInferContext, TypeError,
            func_params::{
                in_out_param::FunctionParamInOut, in_param::FunctionParamIn,
                variadic_param::FunctionParamVariadic,
            },
        },
    },
};

use ::sql_fun_core::IVec;

pub enum ParameterBindResult {
    One(Option<TypeReference>, FunctionParam),
    NoInputParam(FunctionParam),
    VariadicParam(IVec<Option<TypeReference>>, FunctionParam),
}

impl ParameterBindResult {
    pub(crate) fn supplied_variadic(parameter: &FunctionParam, ty: Option<&TypeReference>) -> Self {
        Self::VariadicParam(vec![ty.cloned()].into(), parameter.clone())
    }
    pub(crate) fn supplied(ty: Option<&TypeReference>, parameter: &FunctionParam) -> Self {
        Self::One(ty.cloned(), parameter.clone())
    }
    pub fn one_in(param: &FunctionParamIn, arg: Option<&TypeReference>) -> Self {
        Self::One(arg.cloned(), FunctionParam::from(param))
    }
}

#[cfg(test)]
mod test_one_in {
    use crate::sem::{
        SemScalarExpr, TypeReference,
        type_system::func_params::{bind_result::ParameterBindResult, in_param::FunctionParamIn},
    };

    #[test]
    fn test_one_in() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let default_expr: Option<SemScalarExpr> = None;
        let param = FunctionParamIn::new(name.as_ref(), param_type.as_ref(), default_expr.as_ref());
        let arg = Some(TypeReference::from_master_type_name("int4"));
        let bind = ParameterBindResult::one_in(&param, arg.as_ref());
        assert!(matches!(bind, ParameterBindResult::One(_, _)))
    }
}

impl ParameterBindResult {
    pub fn one_in_out(param: &FunctionParamInOut, arg: Option<&TypeReference>) -> Self {
        Self::One(arg.cloned(), FunctionParam::from(param))
    }
}

#[cfg(test)]
mod test_one_in_out {
    use crate::sem::{
        SemScalarExpr, TypeReference,
        type_system::func_params::{
            bind_result::ParameterBindResult, in_out_param::FunctionParamInOut,
        },
    };

    #[test]
    fn test_one_in_out() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let default_expr: Option<SemScalarExpr> = None;
        let param =
            FunctionParamInOut::new(name.as_ref(), param_type.as_ref(), default_expr.as_ref());
        let arg = Some(TypeReference::from_master_type_name("int4"));
        let bind = ParameterBindResult::one_in_out(&param, arg.as_ref());
        assert!(matches!(bind, ParameterBindResult::One(_, _)))
    }
}

impl ParameterBindResult {
    pub fn variadic(param: &FunctionParamVariadic, args: &IVec<Option<TypeReference>>) -> Self {
        Self::VariadicParam(args.clone(), FunctionParam::from(param))
    }
}

#[cfg(test)]
mod test_variadic {
    use crate::sem::{
        TypeReference,
        type_system::func_params::{
            bind_result::ParameterBindResult, variadic_param::FunctionParamVariadic,
        },
    };

    use ::sql_fun_core::IVec;

    #[test]
    fn test_variadic() {
        let name = Some(String::from("para"));
        let param_type = Some(TypeReference::from_master_type_name("int4"));
        let param = FunctionParamVariadic::new(name.as_ref(), param_type.as_ref());
        let args = vec![Some(TypeReference::from_master_type_name("int4"))];
        let bind = ParameterBindResult::variadic(&param, &IVec::from(args));
        assert!(matches!(bind, ParameterBindResult::VariadicParam(_, _)))
    }
}

impl ParameterBindResult {
    pub fn no_input(param: &FunctionParam) -> Self {
        Self::NoInputParam(param.clone())
    }

    pub(crate) fn constraint_inputs(bindings: &[Self]) -> Option<Vec<TypeReference>> {
        let mut inputs = Vec::new();
        for binding in bindings {
            inputs.extend(binding.constraint_input_types()?);
        }
        Some(inputs)
    }

    pub(crate) fn unrestricted_inputs(bindings: &[Self]) -> Vec<usize> {
        let mut positions = Vec::new();
        let mut offset = 0;
        for binding in bindings {
            match binding {
                Self::One(_, _) => offset += 1,
                Self::NoInputParam(_) => {}
                Self::VariadicParam(arguments, parameter) => {
                    if parameter.accepts_any_type() {
                        positions.extend(offset..offset + arguments.len());
                    }
                    offset += arguments.len();
                }
            }
        }
        positions
    }

    pub(crate) fn constraint_input_types(&self) -> Option<Vec<TypeReference>> {
        match self {
            Self::One(_, parameter) => Some(vec![parameter.get_type()?.clone()]),
            Self::NoInputParam(_) => Some(Vec::new()),
            Self::VariadicParam(arguments, parameter) => {
                let ty = parameter.get_type()?;
                let ty = if ty.is_array() == Some(true) {
                    ty.element_type()
                } else {
                    ty.clone()
                };
                Some(vec![ty; arguments.len()])
            }
        }
    }

    pub(crate) fn overload_rank(
        &self,
        catalog: &impl BaseParseContext,
        declaration: &Self,
    ) -> Option<OverloadRank> {
        use crate::sem::overload_rank::OverloadRank;
        let declared_target = declaration.call_argument_type();
        match self {
            Self::One(source, parameter) => OverloadRank::function_argument(
                catalog,
                source.as_ref(),
                parameter.get_type(),
                declared_target.as_ref(),
            ),
            Self::NoInputParam(_) => Some(OverloadRank::default()),
            Self::VariadicParam(_, parameter) if parameter.accepts_any_type() => {
                Some(OverloadRank::default())
            }
            Self::VariadicParam(arguments, _) => {
                let targets = self.constraint_input_types()?;
                let mut rank = OverloadRank::default();
                for (source, target) in arguments.into_iter().zip(&targets) {
                    rank = rank
                        + OverloadRank::function_argument(
                            catalog,
                            source.as_ref(),
                            Some(target),
                            declared_target.as_ref(),
                        )?;
                }
                Some(rank)
            }
        }
    }

    fn call_argument_type(&self) -> Option<TypeReference> {
        match self {
            Self::One(_, parameter) => parameter.get_type().cloned(),
            Self::VariadicParam(_, parameter) => parameter.call_argument_type(),
            Self::NoInputParam(_) => None,
        }
    }

    pub fn get_arg_types(&self) -> Vec<TypeReference> {
        match self {
            Self::One(t, _) => t.iter().cloned().collect(),
            Self::NoInputParam(_) => Vec::default(),
            Self::VariadicParam(v, _) => v.iter().flatten().cloned().collect(),
        }
    }

    /// collect simple inference parameters
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if inference fails.
    pub fn collect_simple(
        &self,
        context: &mut SimpleInferContext,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        match self {
            Self::One(_, fp) | Self::NoInputParam(fp) | Self::VariadicParam(_, fp) => {
                fp.collect_simple(context, catalog, self)
            }
        }
    }

    /// collect common infer parameters
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if inference fails.
    pub fn collect_common(
        &self,
        context: &mut CommonInferContext,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        match self {
            Self::One(_, fp) | Self::NoInputParam(fp) | Self::VariadicParam(_, fp) => {
                fp.collect_common(context, catalog, self)
            }
        }
    }

    /// test castability score for the bound arguments
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] when implicit casts cannot be resolved.
    pub fn test_castability<TParseContext>(&self, context: &TParseContext) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        self.test_catalog_castability(context)
    }

    pub(crate) fn test_catalog_castability<TCatalog: BaseParseContext>(
        &self,
        context: &TCatalog,
    ) -> Result<i32, TypeError> {
        match self {
            Self::One(arg, p) => Ok(p.castability_one_from_catalog(context, arg)?),
            Self::NoInputParam(_) => Ok(0),
            Self::VariadicParam(args, p) => Ok(p.castability_variadic_from_catalog(context, args)?),
        }
    }
}
