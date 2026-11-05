//! Logical SQL identities shared by syntax, catalogs and semantic analysis.
//!
//! Components preserve logical spelling; SQL parsing and semantic diagnostics
//! remain with their dialect and analysis adapters.

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
use crate::{ast::Name, sem::AnalysisError};

/// A logical SQL source alias.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AliasName(String);

impl std::fmt::Display for AliasName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AliasName {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

impl AliasName {
    /// Return the logical alias spelling without SQL delimiters.
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AliasName {
    pub(crate) fn from_ast_star_qualifier(ast: &Name) -> Result<Option<Self>, AnalysisError> {
        match ast.parts() {
            [] => Ok(None),
            [alias] => Ok(Some(Self::from(alias.as_str()))),
            _ => AnalysisError::raise_unexpected_input("star qualification"),
        }
    }
}
