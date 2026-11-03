mod config_error;
mod control_file;
mod extension_collection;
mod name_version_pair;
#[cfg(test)]
mod test_helpers;

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

impl ExtensionVersion {
    fn installation(version: &str) -> Result<Self, ExtensionConfigError> {
        if version.is_empty()
            || version.contains("--")
            || version.contains('\0')
            || version.starts_with('-')
            || version.ends_with('-')
        {
            return ExtensionConfigError::invalid_extension_version(version);
        }
        Self::try_from(version)
    }
}

#[cfg(test)]
mod test_installation_version {
    use crate::extensions::ExtensionVersion;

    #[test]
    fn validates_postgres_installation_version_names() {
        for invalid in [
            "", "1--2", "-1", "1-", "../1", "a/b", "a\\b", "a:b", "..", ".", "1\0",
        ] {
            assert!(
                ExtensionVersion::installation(invalid).is_err(),
                "{invalid:?}"
            );
        }
        for valid in ["1.1", "release-2", "v_3", "1.0beta", "a'b"] {
            assert_eq!(
                ExtensionVersion::installation(valid).unwrap().to_string(),
                valid
            );
        }
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
