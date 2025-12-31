use std::fmt::Display;

/// part of name specifies catalog
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CatalogName(String);

impl CatalogName {
    /// create from string
    pub fn from(value: &str) -> Self {
        Self(value.to_string())
    }
    /// convert to string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for CatalogName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl Display for CatalogName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
