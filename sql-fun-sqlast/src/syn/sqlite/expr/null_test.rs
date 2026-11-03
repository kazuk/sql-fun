#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;

/// is null expression
#[derive(Clone, Debug)]
pub struct ExprIsNull(Box<Expr>);

impl ExprIsNull {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self(Box::new(expr))
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.0.as_ref()
    }
}

impl Expr {
    /// create IsNull variant
    pub fn is_null<Value>(value: Value) -> Self
    where
        Value: Into<Self>,
    {
        Self::IsNull(ExprIsNull::new(value.into()))
    }
}

/// not null expression
#[derive(Clone, Debug)]
pub struct ExprNotNull(Box<Expr>);

impl ExprNotNull {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self(Box::new(expr))
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.0.as_ref()
    }
}

impl Expr {
    /// create NotNull variant
    pub fn not_null<Value>(value: Value) -> Self
    where
        Value: Into<Self>,
    {
        Self::NotNull(ExprNotNull::new(value.into()))
    }
}
