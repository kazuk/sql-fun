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
#[derive(Debug, serde::Deserialize, Clone, Default)]
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
    pub fn validate_name(name: &str) -> Result<(), ExtensionConfigError> {
        if name == "." || name == ".." || name.contains([':', '/', '\\']) {
            ExtensionConfigError::invalid_extension_name(name)?
        };
        Ok(())
    }

    /// creates instance
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

    /// get extension sql path
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

    /// parse name from str
    ///
    /// returns (parsed_name, remaining)
    ///
    /// Note: parsed name was not strictly checked use [Self::validate_name]
    ///
    pub fn parse_name(expect_name: &str) -> Result<(String, &str), String> {
        let parse_result = Self::name_parser().easy_parse(expect_name);
        parse_result.map_err(|e| e.to_string())
    }

    /// parse version from str
    ///
    /// returns (parsed_version, remaining)
    ///
    pub fn parse_version(input: &str) -> Result<(String, &str), String> {
        Self::version_parser()
            .easy_parse(input)
            .map_err(|e| e.to_string())
    }
}
