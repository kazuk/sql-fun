#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;

/// parenthesized expression
#[derive(Clone, Debug)]
pub struct Parenthesized(Vec<Expr>);

impl Parenthesized {
    /// create instance
    pub const fn new(items: Vec<Expr>) -> Self {
        Self(items)
    }

    /// expression
    pub fn expr(&self) -> &[Expr] {
        self.0.as_ref()
    }
}

impl Expr {
    /// create Parenthesized variant
    pub fn parenthesized<Ex>(items: Vec<Ex>) -> Self
    where
        Ex: Into<Self>,
    {
        Self::Parenthesized(Parenthesized::new(
            items.into_iter().map(|v| v.into()).collect(),
        ))
    }
}
