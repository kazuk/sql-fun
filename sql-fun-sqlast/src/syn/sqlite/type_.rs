use turso_parser::ast::Type as TypeIn;

/// type
#[derive(Debug)]
pub struct Type {
    name: String,
    size: Option<TypeSize>,
}

impl From<TypeIn> for Type {
    fn from(value: TypeIn) -> Self {
        Self {
            name: value.name,
            size: value.size.map(TypeSize::from),
        }
    }
}

impl Type {
    /// get a name
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// get size
    pub fn size(&self) -> Option<&TypeSize> {
        self.size.as_ref()
    }
}

use turso_parser::ast::TypeSize as TypeSizeIn;

use crate::syn::sqlite::Expr;

/// type size value
#[derive(Debug)]
pub enum TypeSize {
    /// max size
    MaxSize(Box<Expr>),
    /// type size
    TypeSize(Box<Expr>, Box<Expr>),
}

impl From<TypeSizeIn> for TypeSize {
    fn from(value: TypeSizeIn) -> Self {
        match value {
            TypeSizeIn::MaxSize(expr) => Self::MaxSize(Box::new(Expr::from(*expr))),
            TypeSizeIn::TypeSize(expr, expr1) => {
                Self::TypeSize(Box::new(Expr::from(*expr)), Box::new(Expr::from(*expr1)))
            }
        }
    }
}
