mod binary;
mod binary_all;
mod binary_any;
mod left_unary;
mod right_unary;

pub use self::{
    binary::{BinaryOpCallSite, UndefinedOperatorCallSite},
    binary_all::{AllBinaryCallSite, UndefinedOperatorAllCallSite},
    binary_any::{AnyBinaryCallSite, UndefinedOperatorAnyCallSite},
    left_unary::LeftUnaryCallSite,
    right_unary::RightUnaryCallSite,
};
use crate::sem::SemScalarExpr;

fn binary_is_not_null(left: &SemScalarExpr, right: &SemScalarExpr) -> Option<bool> {
    if let Some(l) = left.is_not_null() {
        if !l {
            Some(false)
        } else if let Some(r) = right.is_not_null() {
            if r { Some(true) } else { Some(false) }
        } else {
            None
        }
    } else {
        None
    }
}
