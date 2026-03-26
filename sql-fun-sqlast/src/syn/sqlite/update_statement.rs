use turso_parser::ast::Update as UpdateIn;

use crate::syn::sqlite::{Expr, Indexed, Name, QualifiedName, ResolveType, With};

/// update statement
#[derive(derive_getters::Getters)]
pub struct Update {
    with: Option<With>,
    or_conflict: Option<ResolveType>,
    table_name: QualifiedName,
    indexed: Option<Indexed>,
    sets: Vec<Set>,
}

impl Update {
    /// create instance
    pub fn new(
        with: Option<With>,
        or_conflict: Option<ResolveType>,
        table_name: QualifiedName,
        indexed: Option<Indexed>,
        sets: Vec<Set>,
    ) -> Self {
        Self {
            with,
            or_conflict,
            table_name,
            indexed,
            sets,
        }
    }
}

impl From<UpdateIn> for Update {
    fn from(value: UpdateIn) -> Self {
        Self::new(
            value.with.map(With::from),
            value.or_conflict.map(ResolveType::from),
            QualifiedName::from(value.tbl_name),
            value.indexed.map(Indexed::from),
            value.sets.into_iter().map(Set::from).collect(),
        )
    }
}

use turso_parser::ast::Set as SetIn;

/// set clause of update statement
#[derive(derive_getters::Getters)]
pub struct Set {
    col_names: Vec<Name>,
    expr: Box<Expr>,
}

impl From<SetIn> for Set {
    fn from(value: SetIn) -> Self {
        Self {
            col_names: value.col_names.into_iter().map(Name::from).collect(),
            expr: Box::new(Expr::from(*value.expr)),
        }
    }
}
