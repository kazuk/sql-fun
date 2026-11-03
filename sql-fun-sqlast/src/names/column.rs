use std::fmt::Display;

/// column name
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnName(String);

impl From<&Self> for ColumnName {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

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
