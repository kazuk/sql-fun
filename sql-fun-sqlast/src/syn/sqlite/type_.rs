use ::turso_parser::ast::Type as TypeIn;

/// type
#[derive(Clone, Debug)]
pub struct Type {
    name: String,
    size: Option<TypeSize>,
    array_dimensions: u32,
}

impl From<TypeIn> for Type {
    fn from(value: TypeIn) -> Self {
        Self {
            name: value.name,
            size: value.size.map(TypeSize::from),
            array_dimensions: value.array_dimensions,
        }
    }
}

impl Type {
    /// Number of array dimensions explicitly supplied by the parser.
    pub const fn array_dimensions(&self) -> u32 {
        self.array_dimensions
    }

    /// get a name
    pub const fn name(&self) -> &str {
        self.name.as_str()
    }

    /// get size
    pub const fn size(&self) -> Option<&TypeSize> {
        self.size.as_ref()
    }
}

#[cfg(test)]
mod test_type_conversion {
    use crate::test_helpers::SqliteSynBuilder;

    #[test]
    fn preserves_array_dimensions_and_type_size() {
        for (sql_type, dimensions) in [("TEXT", 0), ("TEXT[]", 1), ("TEXT[][]", 2)] {
            let statement =
                SqliteSynBuilder.parse_as_stmt(&format!("CREATE TABLE items (value {sql_type})"));
            let table = statement.as_create_table().unwrap();
            let columns = table.body().as_columns_and_constraint().unwrap();
            let ty = columns.columns()[0].column_type().unwrap();
            assert_eq!(ty.name(), "TEXT");
            assert_eq!(ty.array_dimensions(), dimensions);
            assert!(ty.size().is_none());
        }
        let statement =
            SqliteSynBuilder.parse_as_stmt("CREATE TABLE items (value DECIMAL(8, 2)[][])");
        let table = statement.as_create_table().unwrap();
        let columns = table.body().as_columns_and_constraint().unwrap();
        let ty = columns.columns()[0].column_type().unwrap();
        assert_eq!(ty.name(), "DECIMAL");
        assert_eq!(ty.array_dimensions(), 2);
        assert!(ty.size().is_some());
    }
}

use turso_parser::ast::TypeSize as TypeSizeIn;

use crate::syn::sqlite::Expr;

/// type size value
#[derive(Clone, Debug)]
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
