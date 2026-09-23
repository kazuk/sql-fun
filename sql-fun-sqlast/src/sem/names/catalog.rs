use std::fmt::Display;

#[cfg(feature = "sqlite")]
use crate::sem::AnalysisError;

/// part of name specifies catalog
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CatalogName(String);

impl CatalogName {
    /// create from string
    #[must_use]
    pub fn from(value: &str) -> Self {
        Self(value.to_string())
    }
    /// convert to string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&crate::syn::sqlite::Name> for CatalogName {
    type Error = AnalysisError;

    fn try_from(value: &crate::syn::sqlite::Name) -> Result<Self, Self::Error> {
        Ok(Self(String::from(value.value())))
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
