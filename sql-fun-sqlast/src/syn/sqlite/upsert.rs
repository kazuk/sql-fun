use crate::syn::sqlite::{Expr, Set, SortedColumn};

use turso_parser::ast::Upsert as UpsertIn;

/// upsert statement
#[derive(derive_getters::Getters)]
pub struct Upsert {
    index: Option<UpsertIndex>,
    do_clause: UpsertDo,
    next: Option<Box<Upsert>>,
}

impl From<UpsertIn> for Upsert {
    fn from(value: UpsertIn) -> Self {
        Upsert {
            index: value.index.map(UpsertIndex::from),
            do_clause: UpsertDo::from(value.do_clause),
            next: value.next.map(|v| Box::new(Upsert::from(*v))),
        }
    }
}

use turso_parser::ast::UpsertIndex as UpsertIndexIn;

/// upsert index clause
#[derive(derive_getters::Getters)]
pub struct UpsertIndex {
    targets: Vec<SortedColumn>,
    where_clause: Option<Box<Expr>>,
}

impl From<UpsertIndexIn> for UpsertIndex {
    fn from(value: UpsertIndexIn) -> Self {
        Self {
            targets: value.targets.into_iter().map(SortedColumn::from).collect(),
            where_clause: value.where_clause.map(|e| Box::new(Expr::from(*e))),
        }
    }
}

use turso_parser::ast::UpsertDo as UpsertDoIn;

/// do clause
pub enum UpsertDo {
    /// set sub-clause
    Set(UpsertDoSet),
    /// nothing
    Nothing,
}

impl From<UpsertDoIn> for UpsertDo {
    fn from(value: UpsertDoIn) -> Self {
        match value {
            UpsertDoIn::Set { sets, where_clause } => Self::Set(UpsertDoSet::new(
                sets.into_iter().map(Set::from).collect(),
                where_clause.map(|v| Expr::from(*v)),
            )),
            UpsertDoIn::Nothing => Self::Nothing,
        }
    }
}

/// set clause for upsert
#[derive(derive_getters::Getters)]
pub struct UpsertDoSet {
    sets: Vec<Set>,
    where_clause: Option<Box<Expr>>,
}

impl UpsertDoSet {
    /// create instance
    pub fn new(sets: Vec<Set>, where_clause: Option<Expr>) -> Self {
        Self {
            sets,
            where_clause: where_clause.map(Box::new),
        }
    }
}
