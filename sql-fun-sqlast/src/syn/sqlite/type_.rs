use crate::engine_specific::TursoArrayTypeFeature;
use crate::sem::AnalysisError;

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
        let TypeIn {
            name,
            size,
            array_dimensions,
        } = value;
        Self {
            name,
            size: size.map(TypeSize::from),
            array_dimensions,
        }
    }
}

#[cfg(test)]
mod test_type_conversion {
    use crate::test_helpers::{SqliteArrayTypeGate, SqliteSynBuilder};

    #[rstest::rstest]
    #[case::scalar("TEXT", 0)]
    #[case::one_dimension("TEXT[]", 1)]
    #[case::two_dimensions("TEXT[][]", 2)]
    fn preserves_array_dimensions_without_type_size(
        #[case] sql_type: &str,
        #[case] dimensions: u32,
    ) {
        let statement =
            SqliteSynBuilder.parse_as_stmt(&format!("CREATE TABLE items (value {sql_type})"));
        let table = statement.as_create_table().unwrap();
        let columns = table.body().as_columns_and_constraint().unwrap();
        let ty = columns.columns()[0].column_type().unwrap();
        assert_eq!(ty.name(), "TEXT");
        let mut gate = SqliteArrayTypeGate::default();
        assert_eq!(ty.array_dimensions(&mut gate).unwrap(), dimensions);
        assert_eq!(gate.uses(), usize::from(dimensions > 0));
        assert!(ty.size().is_none());
    }

    #[test]
    fn preserves_array_dimensions_with_type_size() {
        let statement =
            SqliteSynBuilder.parse_as_stmt("CREATE TABLE items (value DECIMAL(8, 2)[][])");
        let table = statement.as_create_table().unwrap();
        let columns = table.body().as_columns_and_constraint().unwrap();
        let ty = columns.columns()[0].column_type().unwrap();
        assert_eq!(ty.name(), "DECIMAL");
        assert_eq!(
            ty.array_dimensions(&mut SqliteArrayTypeGate::default())
                .unwrap(),
            2
        );
        assert!(ty.size().is_some());
    }
}

impl Type {
    /// Parser-provided array dimensions, notifying the Turso hook for array types.
    pub fn array_dimensions<Gate: TursoArrayTypeFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<u32, AnalysisError> {
        if self.array_dimensions > 0 {
            gate.array_type_used(self)?;
        }
        Ok(self.array_dimensions)
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
