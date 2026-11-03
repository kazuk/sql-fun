#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;
use turso_parser::ast::Expr as ExprIn;

/// case when
#[derive(Clone, Debug)]
pub struct CaseWhenThenPair(Box<Expr>, Box<Expr>);

impl From<(Box<ExprIn>, Box<ExprIn>)> for CaseWhenThenPair {
    fn from(value: (Box<ExprIn>, Box<ExprIn>)) -> Self {
        Self::new(Expr::from(*value.0), Expr::from(*value.1))
    }
}

impl CaseWhenThenPair {
    /// create instance
    pub fn new(when: Expr, then: Expr) -> Self {
        Self(Box::new(when), Box::new(then))
    }

    /// when expression
    pub fn when(&self) -> &Expr {
        self.0.as_ref()
    }

    /// then expression
    pub fn then(&self) -> &Expr {
        self.1.as_ref()
    }
}

/// case expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprCase {
    base: Option<Box<Expr>>,
    when_then_pairs: Vec<CaseWhenThenPair>,
    else_expr: Option<Box<Expr>>,
}

impl ExprCase {
    /// create instance
    pub fn new(
        base: Option<Expr>,
        when_then_pairs: Vec<(Box<ExprIn>, Box<ExprIn>)>,
        else_expr: Option<Expr>,
    ) -> Self {
        Self {
            base: base.map(Box::new),
            when_then_pairs: when_then_pairs
                .into_iter()
                .map(CaseWhenThenPair::from)
                .collect(),
            else_expr: else_expr.map(Box::new),
        }
    }
}

impl Expr {
    pub(super) fn case(
        base: Option<Box<ExprIn>>,
        when_then_pairs: Vec<(Box<ExprIn>, Box<ExprIn>)>,
        else_expr: Option<Box<ExprIn>>,
    ) -> Self {
        Self::Case(ExprCase::new(
            base.map(|e| Self::from(*e)),
            when_then_pairs,
            else_expr.map(|e| Self::from(*e)),
        ))
    }
}
