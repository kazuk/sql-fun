//! Logical SQL identities shared by syntax, catalogs and semantic analysis.
//!
//! Components preserve logical spelling; SQL parsing and semantic diagnostics
//! remain with their dialect and analysis adapters.

mod alias;
mod catalog;
mod column;
mod cte;
mod full;
mod local;
mod ope;
mod schema;
mod table;
mod view;

pub use self::{
    alias::AliasName,
    catalog::CatalogName,
    column::ColumnName,
    cte::CteName,
    full::FullName,
    local::LocalName,
    ope::{OperatorName, ValidOperatorName},
    schema::SchemaName,
    table::TableName,
    view::ViewName,
};
