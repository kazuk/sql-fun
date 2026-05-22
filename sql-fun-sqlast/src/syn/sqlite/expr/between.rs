use super::Expr;

/// between expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprBetween {
    lhs: Box<Expr>,
    not: bool,
    start: Box<Expr>,
    end: Box<Expr>,
}

impl ExprBetween {
    /// create instance
    pub fn new(lhs: Expr, not: bool, start: Expr, end: Expr) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            start: Box::new(start),
            end: Box::new(end),
        }
    }
}

impl Expr {
    /// create between variant
    pub fn between<LHS, START, END>(lhs: LHS, not: bool, start: START, end: END) -> Self
    where
        LHS: Into<Expr>,
        START: Into<Expr>,
        END: Into<Expr>,
    {
        Self::Between(ExprBetween::new(lhs.into(), not, start.into(), end.into()))
    }
}
