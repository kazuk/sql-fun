use crate::syn::sqlite::{Expr, Name, QualifiedName, SortedColumn};

/// create index statement
#[derive(derive_getters::Getters)]
pub struct CreateIndexStatement {
    unique: bool,
    if_not_exists: bool,
    idx_name: QualifiedName,
    table_name: Name,
    using: Option<Name>,
    columns: Vec<SortedColumn>,
    with_clause: Vec<(Name, Box<Expr>)>,
    where_clause: Option<Box<Expr>>,
}

impl CreateIndexStatement {
    /// create instance
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        unique: bool,
        if_not_exists: bool,
        idx_name: QualifiedName,
        table_name: Name,
        using: Option<Name>,
        columns: Vec<SortedColumn>,
        with_clause: Vec<(Name, Expr)>,
        where_clause: Option<Expr>,
    ) -> Self {
        Self {
            unique,
            if_not_exists,
            idx_name,
            table_name,
            using,
            columns,
            with_clause: with_clause
                .into_iter()
                .map(|v| (v.0, Box::new(v.1)))
                .collect(),
            where_clause: where_clause.map(Box::new),
        }
    }
}

/// drop index statement
#[derive(derive_getters::Getters)]
pub struct DropIndexStatement {
    if_exists: bool,
    idx_name: QualifiedName,
}

impl DropIndexStatement {
    /// create instance
    pub fn new(if_exists: bool, idx_name: QualifiedName) -> Self {
        Self {
            if_exists,
            idx_name,
        }
    }
}

/// reindex statement
#[derive(derive_getters::Getters)]
pub struct ReindexStatement {
    idx_name: Option<QualifiedName>,
}

impl ReindexStatement {
    /// create instance
    pub fn new(idx_name: Option<QualifiedName>) -> Self {
        Self { idx_name }
    }
}

/// optimize statement
#[derive(derive_getters::Getters)]
pub struct OptimizeStatement {
    idx_name: Option<QualifiedName>,
}

impl OptimizeStatement {
    /// create instance
    pub fn new(idx_name: Option<QualifiedName>) -> Self {
        Self { idx_name }
    }
}
