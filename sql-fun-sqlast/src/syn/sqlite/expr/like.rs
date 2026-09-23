use super::Expr;

/// like expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprLike {
    lhs: Box<Expr>,
    not: bool,
    op: LikeOperator,
    rhs: Box<Expr>,
    escape: Option<Box<Expr>>,
}

impl ExprLike {
    /// create instance
    pub fn new(lhs: Expr, not: bool, op: LikeOperator, rhs: Expr, escape: Option<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            op,
            rhs: Box::new(rhs),
            escape: escape.map(Box::new),
        }
    }
}

impl Expr {
    /// create Like variant
    pub fn like<Value, Ope, Pat, Escape>(
        value: Value,
        not: bool,
        op: Ope,
        pat: Pat,
        escape: Option<Escape>,
    ) -> Self
    where
        Value: Into<Self>,
        Ope: Into<LikeOperator>,
        Pat: Into<Self>,
        Escape: Into<Self>,
    {
        Self::Like(ExprLike::new(
            value.into(),
            not,
            op.into(),
            pat.into(),
            escape.map(|v| v.into()),
        ))
    }
}

use turso_parser::ast::LikeOperator as LikeOperatorIn;

/// like operator
#[derive(Clone, Debug)]
pub enum LikeOperator {
    /// glob
    Glob,
    /// like
    Like,
    /// match
    Match,
    /// regular expression
    Regexp,
}

impl From<LikeOperatorIn> for LikeOperator {
    fn from(value: LikeOperatorIn) -> Self {
        match value {
            LikeOperatorIn::Glob => Self::Glob,
            LikeOperatorIn::Like => Self::Like,
            LikeOperatorIn::Match => Self::Match,
            LikeOperatorIn::Regexp => Self::Regexp,
        }
    }
}
