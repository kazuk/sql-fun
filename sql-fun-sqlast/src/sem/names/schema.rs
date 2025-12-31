use std::fmt::Display;

/// part of name specifies schema
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SchemaName(String);

impl SchemaName {
    /// create from string
    pub fn from(value: &str) -> Self {
        Self(value.to_string())
    }

    /// convert to string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// test schema is `pg_catalog`
    pub fn is_pg_catalog(&self) -> bool {
        &self.0 == "pg_catalog"
    }
}

impl Display for SchemaName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
