use crate::syn::sqlite::TableInternalId;

use super::Expr;

/// column expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprColumn {
    database: Option<usize>,
    table: TableInternalId,
    column: usize,
    is_rowid_alias: bool,
}

impl ExprColumn {
    /// create instance
    pub fn new(
        database: Option<usize>,
        table: TableInternalId,
        column: usize,
        is_rowid_alias: bool,
    ) -> Self {
        Self {
            database,
            table,
            column,
            is_rowid_alias,
        }
    }
}

impl Expr {
    /// create Column variant
    pub fn column<TABLE>(
        database: Option<usize>,
        table: TABLE,
        column: usize,
        is_rowid_alias: bool,
    ) -> Self
    where
        TABLE: Into<TableInternalId>,
    {
        Self::Column(ExprColumn::new(
            database,
            table.into(),
            column,
            is_rowid_alias,
        ))
    }
}

/// row-id expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprRowId {
    database: Option<usize>,
    table: TableInternalId,
}

impl ExprRowId {
    /// create instance
    pub fn new(database: Option<usize>, table: TableInternalId) -> Self {
        Self { database, table }
    }
}

impl Expr {
    /// create RowId variant
    pub fn row_id<TABLE>(database: Option<usize>, table: TABLE) -> Self
    where
        TABLE: Into<TableInternalId>,
    {
        Self::RowId(ExprRowId::new(database, table.into()))
    }
}
