#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Name;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;

/// qualified expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct Qualified {
    table: Name,
    column: Name,
}

impl Qualified {
    /// create instance
    pub const fn new(table: Name, column: Name) -> Self {
        Self { table, column }
    }
}

impl Expr {
    /// create Qualified variant
    pub fn qualified<TABLE, COLUMN>(table: TABLE, column: COLUMN) -> Self
    where
        TABLE: Into<Name>,
        COLUMN: Into<Name>,
    {
        Self::Qualified(Qualified::new(table.into(), column.into()))
    }
}

/// doubly qualified expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprDoublyQualified {
    schema: Name,
    table: Name,
    column: Name,
}

impl ExprDoublyQualified {
    /// create instance
    pub const fn new(schema: Name, table: Name, column: Name) -> Self {
        Self {
            schema,
            table,
            column,
        }
    }
}

impl Expr {
    /// create DoublyQualified variant
    pub fn doubly_qualified<SCHEMA, TABLE, COLUMN>(
        schema: SCHEMA,
        table: TABLE,
        column: COLUMN,
    ) -> Self
    where
        SCHEMA: Into<Name>,
        TABLE: Into<Name>,
        COLUMN: Into<Name>,
    {
        Self::DoublyQualified(ExprDoublyQualified::new(
            schema.into(),
            table.into(),
            column.into(),
        ))
    }
}
