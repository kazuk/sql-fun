use super::Expr;

/// parenthesized expression
#[derive(Clone, Debug)]
pub struct Parenthesized(Vec<Box<Expr>>);

impl Parenthesized {
    /// create instance
    pub fn new(items: Vec<Expr>) -> Self {
        Self(items.into_iter().map(Box::new).collect())
    }

    /// expression
    pub fn expr(&self) -> &[Box<Expr>] {
        self.0.as_ref()
    }
}

impl Expr {
    /// create Parenthesized variant
    pub fn parenthesized<Ex>(items: Vec<Ex>) -> Self
    where
        Ex: Into<Expr>,
    {
        Self::Parenthesized(Parenthesized::new(
            items.into_iter().map(|v| v.into()).collect(),
        ))
    }
}
