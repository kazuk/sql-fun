use std::path::{Path, PathBuf};

use crate::extensions::ExtensionVersion;

/// error in extension configurations
#[derive(thiserror::Error, Debug)]
pub enum ExtensionConfigError {
    /// invalid value for extension name
    #[error("Invalid extension name {0}")]
    InvalidExtensionName(String),

    /// invalid value for extension version
    #[error("Invalid extension version {0}")]
    InvalidExtensionVersion(String),

    /// Invalid extension control file.
    #[error("Invalid extension control file {0}:{1}: {2}")]
    InvalidControlFile(PathBuf, usize, String),

    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// extension SQL not found
    #[error("Extension {0} not found in {1}")]
    ExtensionNotFound(String, PathBuf),

    /// multiple extension version found, require specify one.
    #[error("Extension {0} has multiple versions {1:?}, please specify one of version")]
    MultipleExtensionVersion(String, Vec<String>, PathBuf),

    /// Extension SQL file not existing on path
    #[error("Extension {0}@{1} sql not exist: {2}")]
    ExtensionSqlNotExist(String, ExtensionVersion, PathBuf),
}

impl ExtensionConfigError {
    /// Report invalid extension control syntax or default version.
    ///
    /// # Errors
    /// Always returns an invalid-control-file error.
    pub fn invalid_control_file<T>(path: &Path, line: usize, reason: &str) -> Result<T, Self> {
        Err(Self::InvalidControlFile(
            path.to_path_buf(),
            line,
            reason.to_string(),
        ))
    }

    /// raise `InvalidExtensionName`
    ///
    /// # Errors
    ///
    /// Always returns [`ExtensionConfigError::InvalidExtensionName`].
    pub fn invalid_extension_name<T>(name: &str) -> Result<T, Self> {
        Err(Self::InvalidExtensionName(name.to_string()))
    }

    /// raise `InvalidExtensionVersion`
    ///
    /// # Errors
    ///
    /// Always returns [`ExtensionConfigError::InvalidExtensionVersion`].
    pub fn invalid_extension_version<T>(version: &str) -> Result<T, Self> {
        Err(Self::InvalidExtensionVersion(version.to_string()))
    }

    /// raise `ExtensionNotFound`
    ///
    /// # Errors
    ///
    /// Always returns [`ExtensionConfigError::ExtensionNotFound`].
    pub fn extension_not_found<T, P: AsRef<Path>>(name: &str, base_path: P) -> Result<T, Self> {
        Err(Self::ExtensionNotFound(
            name.to_string(),
            base_path.as_ref().to_path_buf(),
        ))
    }

    /// raise `MultipleExtensionVersion`
    ///
    /// # Errors
    ///
    /// Always returns [`ExtensionConfigError::MultipleExtensionVersion`].
    pub fn multiple_extension_version<T, P: AsRef<Path>>(
        name: &str,
        base_path: P,
        versions: &[String],
    ) -> Result<T, Self> {
        Err(Self::MultipleExtensionVersion(
            name.to_string(),
            versions.iter().map(String::from).collect(),
            base_path.as_ref().to_path_buf(),
        ))
    }

    /// raise `ExtensionSqlNotExist`
    ///
    /// # Errors
    ///
    /// Always returns [`ExtensionConfigError::ExtensionSqlNotExist`].
    pub fn extension_sql_not_exist<T, P: AsRef<Path>>(
        extension_name: &str,
        version: &ExtensionVersion,
        path: P,
    ) -> Result<T, Self> {
        Err(Self::ExtensionSqlNotExist(
            extension_name.to_string(),
            version.clone(),
            path.as_ref().to_path_buf(),
        ))
    }
}

#[cfg(test)]
mod test_extension_config_error {
    use crate::extensions::config_error::ExtensionVersion;

    use std::path::PathBuf;

    use testresult::TestResult;

    use crate::extensions::config_error::ExtensionConfigError;

    #[test]
    fn test_invalid_extension_name() {
        let err: Result<(), _> = ExtensionConfigError::invalid_extension_name("name");
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(ExtensionConfigError::InvalidExtensionName(_))
        ));
    }

    #[test]
    fn test_invalid_extension_version() {
        let err: Result<(), _> = ExtensionConfigError::invalid_extension_version("1.0.0");
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(ExtensionConfigError::InvalidExtensionVersion(_))
        ));
    }

    #[test]
    fn test_extension_not_found() {
        let err: Result<(), _> =
            ExtensionConfigError::extension_not_found("name", PathBuf::from("aaaa"));
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(ExtensionConfigError::ExtensionNotFound(_, _))
        ));
    }

    #[test]
    fn test_multiple_extension_version() {
        let err: Result<(), _> = ExtensionConfigError::multiple_extension_version(
            "name",
            PathBuf::from("aaa"),
            vec![].as_slice(),
        );
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(ExtensionConfigError::MultipleExtensionVersion(_, _, _))
        ));
    }

    #[test]
    fn test_extension_sql_not_exist() -> TestResult {
        let err: Result<(), _> = ExtensionConfigError::extension_sql_not_exist(
            "extension_name",
            &ExtensionVersion::try_from("0.1")?,
            PathBuf::from("aaa"),
        );
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(ExtensionConfigError::ExtensionSqlNotExist(_, _, _))
        ));
        Ok(())
    }
}
