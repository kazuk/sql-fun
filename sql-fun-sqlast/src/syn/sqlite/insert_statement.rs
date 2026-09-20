use crate::syn::sqlite::{Name, QualifiedName, ResolveType, ResultColumn, Select, Upsert, With};

/// insert statement
#[derive(derive_getters::Getters, Debug)]
pub struct InsertStatement {
    with: Option<With>,
    or_conflict: Option<ResolveType>,
    table_name: QualifiedName,
    columns: Vec<Name>,
    body: InsertBody,
    returning: Vec<ResultColumn>,
}

impl InsertStatement {
    /// create instance
    pub const fn new(
        with: Option<With>,
        or_conflict: Option<ResolveType>,
        table_name: QualifiedName,
        columns: Vec<Name>,
        body: InsertBody,
        returning: Vec<ResultColumn>,
    ) -> Self {
        Self {
            with,
            or_conflict,
            table_name,
            columns,
            body,
            returning,
        }
    }
}

use turso_parser::ast::InsertBody as InsertBodyIn;

/// insert statement body
#[derive(Debug)]
pub enum InsertBody {
    /// select
    Select(Box<InsertBodySelect>),
    /// default values
    DefaultValues,
}

impl From<InsertBodyIn> for InsertBody {
    fn from(value: InsertBodyIn) -> Self {
        match value {
            InsertBodyIn::Select(select, upsert) => Self::Select(Box::new(InsertBodySelect::new(
                Select::from(select),
                upsert.map(|u| Upsert::from(*u)),
            ))),
            InsertBodyIn::DefaultValues => Self::DefaultValues,
        }
    }
}

/// select for insert
#[derive(derive_getters::Getters, Debug)]
pub struct InsertBodySelect {
    select: Select,
    upsert: Option<Box<Upsert>>,
}

impl InsertBodySelect {
    /// create instance
    pub fn new(select: Select, upsert: Option<Upsert>) -> Self {
        Self {
            select,
            upsert: upsert.map(Box::new),
        }
    }
}
