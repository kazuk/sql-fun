mod ast;

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
