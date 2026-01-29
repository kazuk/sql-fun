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
#[derive(Debug, serde::Deserialize, Clone, Default)]
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

impl PostgresExtensionsCollection {
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

impl FromStr for PostgresExtensionsCollection {
    type Err = (String, String);

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parse_result = Self::parse(s);
        match parse_result {
            Ok(result) => Ok(Self(IVec::from(result))),
            Err(msg) => Err((msg, s.to_string())),
        }
    }
}
