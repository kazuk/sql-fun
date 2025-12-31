use sql_fun_core::IVec;

use crate::sem::{
    FunctionParam, ParseContext, TypeReference,
    type_system::{
        CommonInferContext, SimpleInferContext, TypeError,
        func_params::{
            in_out_param::FunctionParamInOut, in_param::FunctionParamIn,
            variadic_param::FunctionParamVariadic,
        },
    },
};

pub enum ParameterBindResult {
    One(Option<TypeReference>, FunctionParam),
    NoInputParam(FunctionParam),
    VariadicParam(IVec<Option<TypeReference>>, FunctionParam),
}

impl ParameterBindResult {
    pub fn one_in(param: &FunctionParamIn, arg: &Option<TypeReference>) -> Self {
        Self::One(arg.clone(), FunctionParam::from(param))
    }

    pub fn one_in_out(param: &FunctionParamInOut, arg: &Option<TypeReference>) -> Self {
        Self::One(arg.clone(), FunctionParam::from(param))
    }

    pub fn variadic(param: &FunctionParamVariadic, args: &IVec<Option<TypeReference>>) -> Self {
        Self::VariadicParam(args.clone(), FunctionParam::from(param))
    }

    pub fn no_input(param: &FunctionParam) -> Self {
        Self::NoInputParam(param.clone())
    }

    pub fn get_arg_types(&self) -> Vec<TypeReference> {
        match self {
            Self::One(t, _) => t.iter().cloned().collect(),
            Self::NoInputParam(_) => Vec::default(),
            Self::VariadicParam(v, _) => v.iter().flatten().cloned().collect(),
        }
    }

    /// collect simple inference parameters
    pub fn collect_simple(&self, context: &mut SimpleInferContext) -> Result<(), TypeError> {
        match self {
            Self::One(_, fp) => fp.collect_simple(context, self),
            Self::NoInputParam(fp) => fp.collect_simple(context, self),
            Self::VariadicParam(_, fp) => fp.collect_simple(context, self),
        }
    }

    /// collect common infer parameters
    pub fn collect_common(&self, context: &mut CommonInferContext) -> Result<(), TypeError> {
        match self {
            Self::One(_, fp) => fp.collect_common(context, self),
            Self::NoInputParam(fp) => fp.collect_common(context, self),
            Self::VariadicParam(_, fp) => fp.collect_common(context, self),
        }
    }

    pub fn test_castability<TParseContext>(&self, context: &TParseContext) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        match self {
            Self::One(arg, p) => Ok(p.test_castability_one(context, arg)?),
            Self::NoInputParam(_) => Ok(0),
            Self::VariadicParam(args, p) => Ok(p.test_castability_variadic(context, args)?),
        }
    }
}

#[cfg(test)]
mod test_parameter_bind_result {
    use sql_fun_core::IVec;

    use crate::sem::{
        TypeReference,
        type_system::func_params::{
            in_out_param::FunctionParamInOut, in_param::FunctionParamIn,
            variadic_param::FunctionParamVariadic,
        },
    };

    use super::ParameterBindResult;

    #[test]
    fn test_one_in() {
        let param = FunctionParamIn::new(
            &Some(String::from("para")),
            &Some(TypeReference::from_master_type_name("int4")),
            &None,
        );
        let arg = Some(TypeReference::from_master_type_name("int4"));
        let bind = ParameterBindResult::one_in(&param, &arg);
        assert!(matches!(bind, ParameterBindResult::One(_, _)))
    }

    #[test]
    fn test_one_in_out() {
        let param = FunctionParamInOut::new(
            &Some(String::from("para")),
            &Some(TypeReference::from_master_type_name("int4")),
            &None,
        );
        let arg = Some(TypeReference::from_master_type_name("int4"));
        let bind = ParameterBindResult::one_in_out(&param, &arg);
        assert!(matches!(bind, ParameterBindResult::One(_, _)))
    }

    #[test]
    fn test_variadic() {
        let param = FunctionParamVariadic::new(
            &Some(String::from("para")),
            &Some(TypeReference::from_master_type_name("int4")),
        );
        let args = vec![Some(TypeReference::from_master_type_name("int4"))];
        let bind = ParameterBindResult::variadic(&param, &IVec::from(args));
        assert!(matches!(bind, ParameterBindResult::VariadicParam(_, _)))
    }
}
