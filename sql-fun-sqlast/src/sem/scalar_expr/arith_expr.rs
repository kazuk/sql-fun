mod binary;
mod call_sites;
mod impl_build_op;
mod null_if;
mod unary;

pub use self::{
    call_sites::{
        AllBinaryCallSite, AnyBinaryCallSite, BinaryOpCallSite, LeftUnaryCallSite,
        RightUnaryCallSite, UndefinedOperatorAllCallSite, UndefinedOperatorAnyCallSite,
        UndefinedOperatorCallSite,
    },
    null_if::NullIfExpr,
};

use crate::sem::{SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode};

/// arith expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ArithExpr {
    /// binary operator expression
    OpBinary(Box<BinaryOpCallSite>),
    /// left unary expression
    OpLeftUnary(Box<LeftUnaryCallSite>),
    /// right unary expression
    OpRightUnary(Box<RightUnaryCallSite>),
    /// any binary expression
    OpAnyBinary(Box<AnyBinaryCallSite>),
    /// all binary expression
    OpAllBinary(Box<AllBinaryCallSite>),

    /// undefined operator
    UndefinedOperator(Box<UndefinedOperatorCallSite>),
    /// undefined operator Any
    UndefinedOperatorAny(Box<UndefinedOperatorAnyCallSite>),
    /// undefined operator All
    UndefinedOperatorAll(Box<UndefinedOperatorAllCallSite>),

    /// null-if  expression
    NullIfExpr(Box<NullIfExpr>),

    /// Between
    Between(Box<BetweenExpr>),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BetweenExpr {
    left: SemScalarExpr,
    start: SemScalarExpr,
    end: SemScalarExpr,
    not: bool,
}

impl ArithExpr {
    /// build between expression node
    pub fn between(left: SemScalarExpr, range: (SemScalarExpr, SemScalarExpr)) -> SemScalarExpr {
        SemScalarExpr::Arith(Self::Between(Box::new(BetweenExpr {
            left,
            start: range.0,
            end: range.1,
            not: false,
        })))
    }

    /// build not between expression node
    pub fn not_between(
        left: SemScalarExpr,
        range: (SemScalarExpr, SemScalarExpr),
    ) -> SemScalarExpr {
        SemScalarExpr::Arith(Self::Between(Box::new(BetweenExpr {
            left,
            start: range.0,
            end: range.1,
            not: true,
        })))
    }

    fn inputs_is_not_null(&self) -> Option<bool> {
        match &self {
            Self::OpLeftUnary(call_site) => call_site.is_not_null(),
            Self::OpRightUnary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperator(call_site) => call_site.inputs_is_not_null(),
            Self::OpBinary(call_site) => call_site.is_not_null(),
            Self::OpAnyBinary(call_site) => call_site.is_not_null(),
            Self::OpAllBinary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAny(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAll(call_site) => call_site.is_not_null(),
            Self::NullIfExpr(null_if) => null_if.is_not_null(),
            Self::Between(_) => todo!(),
        }
    }

    const fn operator_returns_null(&self) -> Option<bool> {
        None
    }
}

impl SemScalarExprNode for ArithExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            Self::OpBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpLeftUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpRightUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAnyBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAllBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::NullIfExpr(null_if) => null_if.get_type(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => None,
            Self::Between(_) => todo!(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        let is_strict = match &self {
            Self::OpLeftUnary(site) => site.is_strict(),
            Self::OpRightUnary(site) => site.is_strict(),
            Self::OpBinary(call_site) => call_site.is_strict(),
            Self::OpAnyBinary(call_site) => call_site.is_strict(),
            Self::OpAllBinary(call_site) => call_site.is_strict(),
            Self::NullIfExpr(null_if) => null_if.is_strict(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => return None,
            Self::Between(_) => todo!(),
        };
        if is_strict {
            if matches!(self.inputs_is_not_null(), Some(true)) {
                if matches!(self.operator_returns_null(), Some(false)) {
                    Some(true)
                } else {
                    Some(false)
                }
            } else {
                Some(false)
            }
        } else {
            None
        }
    }
}
