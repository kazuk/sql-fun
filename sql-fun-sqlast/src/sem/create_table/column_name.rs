use std::fmt::Display;

use crate::{sem::AnalysisError, syn::Node};

/// column name
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnName(String);

impl ColumnName {
    /// create from string
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self(String::from(name))
    }

    /// get as &str
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for ColumnName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ColumnName {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

impl Display for ColumnName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<&Node> for ColumnName {
    type Error = AnalysisError;

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        let Some(name) = value.as_string().get_sval() else {
            AnalysisError::raise_unexpected_none("(Node as String).sval")?;
            AnalysisError::never();
        };
        Ok(Self::from(name))
    }
}
