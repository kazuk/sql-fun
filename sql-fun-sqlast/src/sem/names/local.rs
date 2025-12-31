use std::{fmt::Display, ops::Deref};

/// part of name for schema specific local name
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct LocalName(String);

impl LocalName {
    /// create from string
    pub fn from(value: &str) -> Self {
        Self(value.to_string())
    }
    /// convert to string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// local name is record
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
