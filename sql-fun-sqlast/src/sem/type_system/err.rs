use std::convert::Infallible;

use crate::sem::{FunctionParam, TypeReference};

#[derive(thiserror::Error, Debug)]
pub enum TypeError {
    #[error("too many function arguments")]
    TooManyArguments,

    #[error("parameter binding failed , no parameter supplied for parameter {0}")]
    BindFailed(Box<FunctionParam>),

    #[error("simple inference failed, require array type {0}")]
    RequireArray(TypeReference),

    #[error("simple inference failed, require non-array type {0}")]
    RequireNonArray(TypeReference),

    #[error("simple inference failed, require range type {0}")]
    RequireRangeType(TypeReference),

    #[error("simple inference failed, require multi range type {0}")]
    RequireMultiRangeType(TypeReference),

    #[error("simple inference failed, no argument for inference")]
    SimpleInferNoArgs(),

    #[error("simple inference failed, ambiguous")]
    SimpleInferAmbiguous(Vec<TypeReference>),

    #[error("common inference failed : no implicit castable")]
    CommonInferNoImplicitCastable(),

    #[error("common inference failed : ambiguous")]
    CommonInferAmbiguous(),

    #[error("Unexpected error (cast not found in castable list items)")]
    UnexpectedCastNotFound,

    #[error("Implicit cast not found")]
    ImplicitCastNotFound(String, String),
}

impl TypeError {
    pub(crate) const fn too_many_arguments() -> Self {
        Self::TooManyArguments
    }

    pub const fn common_infer_no_implicit_castable() -> Result<Infallible, Self> {
        Err(Self::CommonInferNoImplicitCastable())
    }

    pub const fn unexpected_cast_not_found() -> Result<Infallible, Self> {
        Err(Self::UnexpectedCastNotFound)
    }

    pub const fn common_infer_ambiguous() -> Result<Infallible, Self> {
        Err(Self::CommonInferAmbiguous())
    }

    pub fn implicit_cast_not_found(
        source_type: &str,
        target_type: &str,
    ) -> Result<Infallible, Self> {
        Err(Self::ImplicitCastNotFound(
            String::from(source_type),
            String::from(target_type),
        ))
    }

    pub fn never() -> ! {
        // TODO: remove this function on complete never-type support comes in Rust
        unreachable!("Ok(Infallible)")
    }
}
