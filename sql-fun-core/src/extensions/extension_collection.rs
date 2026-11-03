use std::{
    fmt::{Display, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use combine::{
    EasyParser, ParseError, Parser, Stream, eof,
    parser::char::{char, spaces},
    sep_end_by,
};

use crate::{ExtensionConfigError, ExtensionNameVersionPair, IVec};

/// collection for postgres extensions
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Default)]
#[serde(transparent)]
pub struct PostgresExtensionsCollection(IVec<ExtensionNameVersionPair>);

impl Display for PostgresExtensionsCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, item) in self.0.iter().enumerate() {
            if index == self.0.len() - 1 {
                write!(f, "{item}")?;
            } else {
                write!(f, "{item},")?;
            }
        }
        Ok(())
    }
}

/// parse error
#[derive(Debug)]
pub struct PostgresExtensionsParseError {
    message: String,
    original: String,
}

impl std::fmt::Display for PostgresExtensionsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error: {}\n  {}", self.message, self.original)
    }
}

impl PostgresExtensionsParseError {
    fn new(message: &str, original: &str) -> Self {
        Self {
            message: String::from(message),
            original: String::from(original),
        }
    }
}

impl TryFrom<String> for PostgresExtensionsCollection {
    type Error = PostgresExtensionsParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl FromStr for PostgresExtensionsCollection {
    type Err = PostgresExtensionsParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parse_result = Self::parse(s);
        match parse_result {
            Ok(result) => Ok(Self(IVec::from(result))),
            Err(msg) => Err(PostgresExtensionsParseError::new(&msg, s)),
        }
    }
}

impl PostgresExtensionsCollection {
    pub(crate) fn parse_arg(value: &str) -> Result<Self, String> {
        let result = Self::from_str(value)
            .map_err(|_| String::from("can not to parse extensions collection"))?;
        Ok(result)
    }

    ///
    /// get extension SQL file paths
    ///
    /// guranties returned paths exists true
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError`] when any referenced SQL file cannot be
    /// resolved.
    pub fn sql_files<P: AsRef<Path>>(
        &mut self,
        base_path: P,
    ) -> Result<IVec<PathBuf>, ExtensionConfigError> {
        let mut result = Vec::new();
        for item in &mut self.0 {
            result.push(item.resolve_version(&base_path)?);
        }
        Ok(IVec::from(result))
    }

    /// resolve versions for extensions
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionConfigError`] when extension versions cannot be
    /// determined.
    pub fn resolve_versions<P: AsRef<Path>>(
        &mut self,
        base_path: P,
    ) -> Result<(), ExtensionConfigError> {
        for item in &mut self.0 {
            item.resolve_version(&base_path)?;
        }
        Ok(())
    }

    fn list_of_pair_parser<Input>()
    -> impl Parser<Input, Output = Vec<Result<ExtensionNameVersionPair, ExtensionConfigError>>>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        spaces()
            .with(sep_end_by(
                ExtensionNameVersionPair::parser().skip(spaces()),
                char(',').skip(spaces()),
            ))
            .skip(spaces())
    }

    fn list_of_pair_eof_parser<Input>()
    -> impl Parser<Input, Output = Vec<Result<ExtensionNameVersionPair, ExtensionConfigError>>>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        Self::list_of_pair_parser().skip(eof())
    }

    fn parse(input: &str) -> Result<Vec<ExtensionNameVersionPair>, String> {
        let parsed = Self::list_of_pair_eof_parser()
            .easy_parse(input)
            .map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        let mut errors = String::new();
        for item in parsed.0 {
            match item {
                Ok(v) => results.push(v),
                Err(e) => writeln!(&mut errors, "{e}").expect("writing to string"),
            }
        }
        if errors.is_empty() {
            Ok(results)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod test_uuid_ossp_resolution {
    use crate::extensions::{
        extension_collection::PostgresExtensionsCollection, test_helpers::ExtensionFixture,
    };

    #[test]
    fn resolves_versions_from_controls_with_non_utf8_comments_and_values()
    -> ::testresult::TestResult {
        let fixture = ExtensionFixture::with_control_bytes(
            "uuid-ossp",
            &["1.1", "1.0--1.1"],
            b"# \xff\ncomment='\xfe'\ndefault_version='1.1'",
        )?;
        let mut extensions: PostgresExtensionsCollection = "uuid-ossp".parse().unwrap();
        extensions.resolve_versions(fixture.path())?;
        assert_eq!(extensions.to_string(), "uuid-ossp@1.1");
        assert_eq!(
            extensions.sql_files(fixture.path())?[0],
            fixture.script("uuid-ossp", "1.1")
        );
        Ok(())
    }

    #[test]
    fn resolves_uuid_ossp_with_its_update_script_present() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::uuid_ossp()?;
        let mut extensions: PostgresExtensionsCollection = "uuid-ossp".parse().unwrap();
        extensions.resolve_versions(fixture.path())?;
        assert_eq!(extensions.to_string(), "uuid-ossp@1.1");
        let files = extensions.sql_files(fixture.path())?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], fixture.script("uuid-ossp", "1.1"));
        assert!(fixture.script("uuid-ossp", "1.0--1.1").is_file());
        Ok(())
    }
}
