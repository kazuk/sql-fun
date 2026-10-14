//! Common name types for SQL semantic analysis.
//!
//! This facade reexports catalog, schema, and local name components
//! (`CatalogName`, `SchemaName`, and `LocalName`), qualified names
//! (`FullName`), and operator names (`OperatorName` and `ValidOperatorName`).
//! Their implementations remain in private child modules; consumers access
//! these types through `sql_fun_sqlast::sem`.
//!
//! Reviewed and kept because this facade provides a shared boundary for names
//! used by semantic conversion and resolution.
//! See <https://github.com/kazuk/sql-fun/issues/450>.

mod catalog;
mod full;
mod local;
mod ope;
mod schema;

pub use self::{
    catalog::CatalogName,
    full::FullName,
    local::LocalName,
    ope::{OperatorName, ValidOperatorName},
    schema::SchemaName,
};
