use std::{fmt::Display, ops::Deref};

#[cfg(feature = "sqlite")]
use crate::sem::AnalysisError;

/// part of name for schema specific local name
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct LocalName(String);

impl LocalName {
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

    /// local name is record
    #[must_use]
    pub fn is_record(&self) -> bool {
        &self.0 == "record"
    }
}

impl Display for LocalName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for LocalName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&crate::syn::sqlite::Name> for LocalName {
    type Error = AnalysisError;

    fn try_from(value: &crate::syn::sqlite::Name) -> Result<Self, Self::Error> {
        Ok(Self::from(value.value()))
    }
}
