use combine::{
    EasyParser, ParseError, Parser, Stream, choice, many1,
    parser::char::{alpha_num, char},
    satisfy,
};

use std::{
    fmt::{Display, Write},
    path::{Path, PathBuf},
};

use crate::{ExtensionConfigError, ExtensionVersion};

/// Postgres extensin name version pair.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct ExtensionNameVersionPair {
    name: String,
    version: Option<ExtensionVersion>,
}

impl Display for ExtensionNameVersionPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        if let Some(ver) = &self.version {
            f.write_char('@')?;
            write!(f, "{ver}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_extension_name_version_pair_display {
    use crate::ExtensionVersion;

    #[test]
    fn test_extension_name_version_pair_display() {
        let item = super::ExtensionNameVersionPair {
            name: String::from("ext"),
            version: Some(ExtensionVersion::new("1.0")),
        };
        let display_result = item.to_string();
        assert_eq!("ext@1.0", display_result)
    }
}

impl ExtensionNameVersionPair {
    /// validate str for extension name
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError::InvalidExtensionName`] when the provided
    /// name contains unsupported characters.
    pub fn validate_name(name: &str) -> Result<(), ExtensionConfigError> {
        if name == "." || name == ".." || name.contains([':', '/', '\\']) {
            ExtensionConfigError::invalid_extension_name(name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_validate_name {
    use super::ExtensionNameVersionPair;

    #[test]
    fn test_validate_name_invalid() {
        assert!(ExtensionNameVersionPair::validate_name("..").is_err());
        assert!(ExtensionNameVersionPair::validate_name("name:invalid").is_err());
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(ExtensionNameVersionPair::validate_name("ext_name").is_ok());
    }
}

impl ExtensionNameVersionPair {
    /// creates instance
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError`] if the extension name is invalid.
    pub fn try_new(
        name: &str,
        version: &Option<ExtensionVersion>,
    ) -> Result<Self, ExtensionConfigError> {
        Self::validate_name(name)?;

        Ok(Self {
            name: name.to_string(),
            version: version.clone(),
        })
    }
}

#[cfg(test)]
mod test_try_new {
    use super::ExtensionNameVersionPair;

    #[test]
    fn test_try_new_ok() {
        let version = Some(super::ExtensionVersion::new("1.0"));
        let result = ExtensionNameVersionPair::try_new("ext", &version).unwrap();
        assert_eq!("ext", result.name);
        assert!(result.version.is_some());
    }

    #[test]
    fn test_try_new_invalid_name() {
        let version = None;
        let result = ExtensionNameVersionPair::try_new("..", &version);
        assert!(result.is_err());
    }
}

impl ExtensionNameVersionPair {
    /// get extension sql path
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError`] when the SQL file cannot be located or
    /// metadata resolution fails.
    pub fn resolve_version<P: AsRef<Path>>(
        &mut self,
        base_path: P,
    ) -> Result<PathBuf, ExtensionConfigError> {
        if let Some(version) = &self.version {
            let path = base_path
                .as_ref()
                .join(PathBuf::from(format!("{}--{version}.sql", self.name)));
            if path.exists() {
                Ok(path)
            } else {
                ExtensionConfigError::extension_sql_not_exist(&self.name, version, path)
            }
        } else {
            self.resolve_version_from_extension_files(base_path)
        }
    }

    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError`] when directory traversal fails or when
    /// the version cannot be inferred uniquely.
    fn resolve_version_from_extension_files<P: AsRef<Path>>(
        &mut self,
        base_path: P,
    ) -> Result<PathBuf, ExtensionConfigError> {
        let name_prefix = format!("{}--", self.name);
        let dir = base_path.as_ref().read_dir()?;
        let mut version_candidates = Vec::new();
        for entry in dir {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name_path = PathBuf::from(entry.file_name());
            let Some(ext) = name_path.extension().and_then(|v| v.to_str()) else {
                continue;
            };
            let Some(stem) = name_path.file_stem().and_then(|v| v.to_str()) else {
                continue;
            };
            if !ext.eq_ignore_ascii_case("sql") || !stem.starts_with(&name_prefix) {
                continue;
            }
            let version = &stem[name_prefix.len()..];
            if !version.is_empty() {
                version_candidates.push(version.to_string());
            }
        }

        if version_candidates.len() == 1 {
            let version = &version_candidates[0];
            self.version = Some(ExtensionVersion::new(version));
            let path = base_path
                .as_ref()
                .join(PathBuf::from(format!("{}--{version}.sql", self.name)));
            Ok(path)
        } else if version_candidates.is_empty() {
            ExtensionConfigError::extension_not_found(&self.name, base_path)
        } else {
            ExtensionConfigError::multiple_extension_version(
                &self.name,
                base_path,
                &version_candidates,
            )
        }
    }
}

#[cfg(test)]
mod test_resolve_version {
    use std::fs::File;

    use testresult::TestResult;

    use super::ExtensionNameVersionPair;

    #[test]
    fn test_resolve_version_with_version() -> TestResult {
        let tmp_dir = tempfile::tempdir()?;
        let file_path = tmp_dir.path().join("ext--1.0.sql");
        File::create(&file_path)?;

        let version = Some(super::ExtensionVersion::try_from("1.0")?);
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;
        let resolved = pair.resolve_version(tmp_dir.path())?;

        assert_eq!(file_path, resolved);
        Ok(())
    }

    #[test]
    fn test_resolve_version_missing_file() -> TestResult {
        let tmp_dir = tempfile::tempdir()?;
        let version = Some(super::ExtensionVersion::try_from("1.0")?);
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;

        let result = pair.resolve_version(tmp_dir.path());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_resolve_version_from_extension_files_single() -> TestResult {
        let tmp_dir = tempfile::tempdir()?;
        let file_path = tmp_dir.path().join("ext--1.0.sql");
        File::create(&file_path)?;

        let version = None;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;
        let resolved = pair.resolve_version_from_extension_files(tmp_dir.path())?;

        assert_eq!(file_path, resolved);
        assert_eq!(Some(super::ExtensionVersion::new("1.0")), pair.version);
        Ok(())
    }

    #[test]
    fn test_resolve_version_from_extension_files_multiple() -> TestResult {
        let tmp_dir = tempfile::tempdir()?;
        File::create(tmp_dir.path().join("ext--1.0.sql"))?;
        File::create(tmp_dir.path().join("ext--2.0.sql"))?;

        let version = None;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;
        let result = pair.resolve_version_from_extension_files(tmp_dir.path());

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_resolve_version_from_extension_files_missing() -> TestResult {
        let tmp_dir = tempfile::tempdir()?;

        let version = None;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;
        let result = pair.resolve_version_from_extension_files(tmp_dir.path());

        assert!(result.is_err());
        Ok(())
    }
}

impl ExtensionNameVersionPair {
    fn name_parser<Input>() -> impl Parser<Input, Output = String>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        many1(choice((alpha_num(), char('_'), char('-'), char('.'))))
    }

    fn version_parser<Input>() -> impl Parser<Input, Output = String>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        many1(satisfy(|c: char| c != ',' && !c.is_whitespace()))
    }
}

#[cfg(test)]
mod test_parsers {
    use combine::EasyParser;

    #[test]
    fn test_name_parser() {
        let (parsed, remaining) = super::ExtensionNameVersionPair::name_parser()
            .easy_parse("extension-1.0 rest")
            .unwrap();
        assert_eq!("extension-1.0", parsed);
        assert_eq!(" rest", remaining);
    }

    #[test]
    fn test_version_parser() {
        let (parsed, remaining) = super::ExtensionNameVersionPair::version_parser()
            .easy_parse("1.0,rest")
            .unwrap();
        assert_eq!("1.0", parsed);
        assert_eq!(",rest", remaining);
    }
}

impl ExtensionNameVersionPair {
    pub(crate) fn parser<Input>()
    -> impl Parser<Input, Output = Result<ExtensionNameVersionPair, ExtensionConfigError>>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        (
            Self::name_parser(),
            combine::optional(char('@').with(Self::version_parser())),
        )
            .map(|(name, ver)| {
                let version = ver
                    .map(|v| ExtensionVersion::try_from(v.as_str()))
                    .transpose()?;
                ExtensionNameVersionPair::try_new(&name, &version)
            })
    }
}

#[cfg(test)]
mod test_parser {
    use combine::EasyParser;

    use super::ExtensionNameVersionPair;

    #[test]
    fn test_parser_with_version() {
        let (pair, remaining) = ExtensionNameVersionPair::parser()
            .easy_parse("ext@1.0,rest")
            .unwrap();

        let pair = pair.unwrap();
        assert_eq!("ext", pair.name);
        assert_eq!(Some(super::ExtensionVersion::new("1.0")), pair.version);
        assert_eq!(",rest", remaining);
    }

    #[test]
    fn test_parser_without_version() {
        let (pair, remaining) = ExtensionNameVersionPair::parser()
            .easy_parse("ext rest")
            .unwrap();

        let pair = pair.unwrap();
        assert_eq!("ext", pair.name);
        assert!(pair.version.is_none());
        assert_eq!(" rest", remaining);
    }
}

impl ExtensionNameVersionPair {
    /// parse name from str
    ///
    /// returns (`parsed_name`, remaining)
    ///
    /// Note: parsed name was not strictly checked use [`Self::validate_name`]
    ///
    /// # Errors
    ///
    /// Returns [`String`] when parsing fails.
    pub fn parse_name(expect_name: &str) -> Result<(String, &str), String> {
        let parse_result = Self::name_parser().easy_parse(expect_name);
        parse_result.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod test_parse_name {
    use super::ExtensionNameVersionPair;

    #[test]
    fn test_parse_name_ok() {
        let (parsed, remaining) = ExtensionNameVersionPair::parse_name("ext rest").unwrap();
        assert_eq!("ext", parsed);
        assert_eq!(" rest", remaining);
    }
}

impl ExtensionNameVersionPair {
    /// parse version from str
    ///
    /// returns (`parsed_version`, remaining)
    ///
    /// # Errors
    ///
    /// Returns [`String`] when parsing fails.
    pub fn parse_version(input: &str) -> Result<(String, &str), String> {
        Self::version_parser()
            .easy_parse(input)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod test_parse_version {
    use super::ExtensionNameVersionPair;

    #[test]
    fn test_parse_version_ok() {
        let (parsed, remaining) = ExtensionNameVersionPair::parse_version("1.0 rest").unwrap();
        assert_eq!("1.0", parsed);
        assert_eq!(" rest", remaining);
    }
}
