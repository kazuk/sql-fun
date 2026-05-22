use crate::syn::sqlite::{IndexedColumn, Name, Select};

use turso_parser::ast::With as WithIn;

/// with clause
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct With {
    recursive: bool,
    ctes: Vec<CommonTableExpr>,
}

impl From<WithIn> for With {
    fn from(value: WithIn) -> Self {
        Self {
            recursive: value.recursive,
            ctes: value.ctes.into_iter().map(CommonTableExpr::from).collect(),
        }
    }
}

use turso_parser::ast::CommonTableExpr as CommonTableExprIn;

/// Common table expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct CommonTableExpr {
    table_name: Name,
    columns: Vec<IndexedColumn>,
    materialized: Materialized,
    select: Select,
}

impl From<CommonTableExprIn> for CommonTableExpr {
    fn from(value: CommonTableExprIn) -> Self {
        Self {
            table_name: Name::from(value.tbl_name),
            columns: value.columns.into_iter().map(IndexedColumn::from).collect(),
            materialized: Materialized::from(value.materialized),
            select: Select::from(value.select),
        }
    }
}

use turso_parser::ast::Materialized as MaterializedIn;

/// materialized option value
#[derive(Clone, Debug)]
pub enum Materialized {
    /// Any
    Any,
    /// Yes
    Yes,
    /// No
    No,
}

impl From<MaterializedIn> for Materialized {
    fn from(value: MaterializedIn) -> Self {
        match value {
            MaterializedIn::Any => Self::Any,
            MaterializedIn::Yes => Self::Yes,
            MaterializedIn::No => Self::No,
        }
    }
}
