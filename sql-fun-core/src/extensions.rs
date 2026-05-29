mod config_error;
mod extension_collection;
mod name_version_pair;

pub use self::{
    config_error::ExtensionConfigError,
    extension_collection::{PostgresExtensionsCollection, PostgresExtensionsParseError},
    name_version_pair::ExtensionNameVersionPair,
};

use std::fmt::Display;

/// Version text for `PostgreSQL` extensions
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct ExtensionVersion(String);

impl ExtensionVersion {
    /// create instance
    #[must_use]
    pub fn new(version: &str) -> Self {
        Self(version.to_string())
    }
}

impl TryFrom<&str> for ExtensionVersion {
    type Error = ExtensionConfigError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "." || value == ".." || value.contains([':', '/', '\\']) {
            ExtensionConfigError::invalid_extension_version(value)?;
        }
        Ok(Self(value.to_string()))
    }
}

impl Display for ExtensionVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
