use combine::{
    EasyParser, ParseError, Parser, Stream, choice, many1,
    parser::char::{alpha_num, char},
    satisfy,
};

use std::{
    fmt::{Display, Write},
    path::Path,
};

use crate::{
    ExtensionConfigError, ExtensionScript, ExtensionScriptSettings, ExtensionVersion,
    extensions::script_inventory::{ScriptInventory, script_path},
};

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
    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

    use crate::ExtensionVersion;

    #[test]
    fn test_extension_name_version_pair_display() {
        let item = ExtensionNameVersionPair {
            name: String::from("ext"),
            version: Some(ExtensionVersion::new("1.0")),
        };
        let display_result = item.to_string();
        assert_eq!("ext@1.0", display_result);
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
    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

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
    use crate::extensions::name_version_pair::ExtensionVersion;

    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

    #[test]
    fn test_try_new_ok() {
        let version = Some(ExtensionVersion::new("1.0"));
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
    /// Resolve the ordered installation and update scripts and record the target version.
    ///
    /// # Errors
    /// Returns resolution or control-file errors.
    pub fn resolve_version<P: AsRef<Path>>(
        &mut self,
        directory: P,
    ) -> Result<Vec<ExtensionScript>, ExtensionConfigError> {
        let directory = directory.as_ref();
        Self::validate_name(&self.name)?;
        let mut primary = ExtensionScriptSettings::default();
        primary.apply_file(&directory.join(format!("{}.control", self.name)), false)?;
        let script_directory = primary.script_directory(directory);
        let Some(target) = self.version.as_ref().or(primary.default_version()) else {
            let inventory = ScriptInventory::read(&script_directory, &self.name)?;
            let choices = inventory
                .available_versions()
                .into_iter()
                .filter(|version| version.chars().all(Self::is_version_argument_char))
                .collect::<Vec<_>>();
            let message = if choices.is_empty() {
                "no usable versions found".to_owned()
            } else {
                format!("available versions: {}", choices.join(", "))
            };
            return ExtensionConfigError::missing_default_version(&self.name, message);
        };
        let target = ExtensionVersion::installation(&target.to_string())?;
        let version = target.to_string();
        let direct = script_path(&script_directory, &self.name, &version);
        let scripts = if direct.is_file() {
            vec![ExtensionScript::read(
                &self.name, direct, &version, &primary,
            )?]
        } else {
            let inventory = ScriptInventory::read(&script_directory, &self.name)?;
            let Some(path) = inventory.installation_path(&version) else {
                return ExtensionConfigError::no_installation_path(
                    &self.name,
                    target,
                    script_directory,
                );
            };
            path.into_iter()
                .scan(None, |previous, destination| {
                    let filename_version = previous.replace(destination).map_or_else(
                        || destination.to_owned(),
                        |source| format!("{source}--{destination}"),
                    );
                    let path = script_path(&script_directory, &self.name, &filename_version);
                    Some(ExtensionScript::read(
                        &self.name,
                        path,
                        destination,
                        &primary,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        self.version = Some(target);
        Ok(scripts)
    }
}

#[cfg(test)]
mod test_installation_resolution {
    use crate::{
        ExtensionScript, ExtensionVersion,
        extensions::{name_version_pair::ExtensionNameVersionPair, test_helpers::ExtensionFixture},
    };

    #[test]
    fn prefers_default_and_explicit_version() -> ::testresult::TestResult {
        let fixture =
            ExtensionFixture::new("ext", &["1", "2", "1--2"], Some("default_version='2'"))?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        assert_eq!(
            pair.resolve_version(fixture.path())?[0].path(),
            fixture.script("ext", "2")
        );
        assert_eq!(pair.to_string(), "ext@2");
        let mut explicit =
            ExtensionNameVersionPair::try_new("ext", &Some(ExtensionVersion::new("1")))?;
        assert_eq!(
            explicit.resolve_version(fixture.path())?[0].path(),
            fixture.script("ext", "1")
        );
        Ok(())
    }

    #[test]
    fn missing_default_reports_usable_versions() -> ::testresult::TestResult {
        for control in [None, Some("comment='no default'")] {
            let fixture = ExtensionFixture::new("ext", &["z", "z--b", "b--a", "x--y"], control)?;
            let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
            assert_eq!(
                pair.resolve_version(fixture.path())
                    .unwrap_err()
                    .to_string(),
                "Extension ext requires an explicit version (ext@version); available versions: a, b, z"
            );
            let mut explicit =
                ExtensionNameVersionPair::try_new("ext", &Some(ExtensionVersion::new("a")))?;
            let scripts = explicit.resolve_version(fixture.path())?;
            assert_eq!(
                scripts
                    .iter()
                    .map(ExtensionScript::path)
                    .collect::<Vec<_>>(),
                ["z", "z--b", "b--a"].map(|version| fixture.script("ext", version))
            );
        }
        Ok(())
    }

    #[test]
    fn errors_do_not_infer_a_version() -> ::testresult::TestResult {
        for (versions, control) in [
            (&["1--2"][..], None),
            (&["1", "2"][..], None),
            (&["1"][..], Some("default_version='2'")),
            (&["1"][..], Some("default_version=''")),
            (&["1"][..], Some("default_version='../1'")),
            (&["1--2"][..], Some("default_version='1--2'")),
            (&["1"][..], Some("default_version='unfinished")),
        ] {
            let fixture = ExtensionFixture::new("ext", versions, control)?;
            let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
            assert!(pair.resolve_version(fixture.path()).is_err());
            assert_eq!(pair.to_string(), "ext");
        }
        Ok(())
    }

    #[test]
    fn ignores_script_directories_and_other_extensions() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new("other-ext", &["1"], None)?;
        fixture.script_directory("ext", "1")?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        assert!(pair.resolve_version(fixture.path()).is_err());
        Ok(())
    }

    #[test]
    fn explicit_selection_reads_control_and_rejects_update_filenames() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new("ext", &["1", "1--2"], Some("invalid"))?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &Some(ExtensionVersion::new("1")))?;
        assert!(pair.resolve_version(fixture.path()).is_err());
        let fixture = ExtensionFixture::new("ext", &["1", "1--2"], None)?;
        let mut update =
            ExtensionNameVersionPair::try_new("ext", &Some(ExtensionVersion::new("1--2")))?;
        assert!(update.resolve_version(fixture.path()).is_err());
        Ok(())
    }

    #[test]
    fn update_path_uses_postgres_ties_and_destination_controls() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new(
            "ext",
            &["1", "2", "1--x", "2--x", "2--y", "x--3", "y--3", "3--4"],
            Some("default_version='4'\nrequires='primary'"),
        )?;
        std::fs::write(
            fixture.path().join("ext--3.control"),
            r#"requires='CuBe, "Quoted Prerequisite"'
module_pathname='update'"#,
        )?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        let scripts = pair.resolve_version(fixture.path())?;
        assert_eq!(
            scripts
                .iter()
                .map(ExtensionScript::path)
                .collect::<Vec<_>>(),
            ["2", "2--x", "x--3", "3--4"].map(|version| fixture.script("ext", version))
        );
        assert_eq!(
            scripts[2].settings().requires(),
            ["cube", "Quoted Prerequisite"]
        );
        assert_eq!(scripts[2].settings().module_pathname(), Some("update"));
        assert_eq!(scripts[3].settings().requires(), ["primary"]);
        assert_eq!(scripts[3].settings().module_pathname(), None);
        assert_eq!(pair.to_string(), "ext@4");
        Ok(())
    }

    #[test]
    fn reads_relative_script_directory_and_rejects_auxiliary_directory() -> ::testresult::TestResult
    {
        let root = ::tempfile::tempdir()?;
        let controls = root.path().join("extension");
        let scripts = root.path().join("scripts");
        std::fs::create_dir(&controls)?;
        std::fs::create_dir(&scripts)?;
        std::fs::write(
            controls.join("ext.control"),
            "directory='scripts'\ndefault_version='1'",
        )?;
        let sql = scripts.join("ext--1.sql");
        std::fs::write(&sql, "")?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        assert_eq!(pair.resolve_version(&controls)?[0].path(), sql);
        std::fs::write(scripts.join("ext--1.control"), "directory='other'")?;
        assert!(
            pair.resolve_version(&controls)
                .unwrap_err()
                .to_string()
                .contains("primary control settings")
        );
        Ok(())
    }

    #[test]
    fn propagates_control_read_errors() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new("ext", &["1"], None)?;
        fixture.unreadable_control("ext")?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        assert!(pair.resolve_version(fixture.path()).is_err());
        assert_eq!(pair.to_string(), "ext");
        Ok(())
    }
    #[test]
    fn test_resolve_version_with_version() -> ::testresult::TestResult {
        let tmp_dir = tempfile::tempdir()?;
        let file_path = tmp_dir.path().join("ext--1.0.sql");
        std::fs::File::create(&file_path)?;

        let version = Some(ExtensionVersion::try_from("1.0")?);
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;
        let resolved = pair.resolve_version(tmp_dir.path())?;

        assert_eq!(file_path, resolved[0].path());
        Ok(())
    }

    #[test]
    fn test_resolve_version_missing_file() -> ::testresult::TestResult {
        let tmp_dir = tempfile::tempdir()?;
        let version = Some(ExtensionVersion::try_from("1.0")?);
        let mut pair = ExtensionNameVersionPair::try_new("ext", &version)?;

        let result = pair.resolve_version(tmp_dir.path());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn missing_default_requires_explicit_version_with_zero_or_one_choice()
    -> ::testresult::TestResult {
        let directory = tempfile::tempdir()?;
        let mut pair = ExtensionNameVersionPair::try_new("ext", &None)?;
        assert!(
            pair.resolve_version(directory.path())
                .unwrap_err()
                .to_string()
                .ends_with("no usable versions found")
        );
        std::fs::File::create(directory.path().join("ext--1.0.sql"))?;
        assert!(
            pair.resolve_version(directory.path())
                .unwrap_err()
                .to_string()
                .ends_with("available versions: 1.0")
        );
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

    const fn is_version_argument_char(character: char) -> bool {
        character != ',' && !character.is_whitespace()
    }

    fn version_parser<Input>() -> impl Parser<Input, Output = String>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        many1(satisfy(Self::is_version_argument_char))
    }
}

#[cfg(test)]
mod test_parsers {
    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

    use combine::EasyParser;

    #[test]
    fn test_name_parser() {
        let (parsed, remaining) = ExtensionNameVersionPair::name_parser()
            .easy_parse("extension-1.0 rest")
            .unwrap();
        assert_eq!("extension-1.0", parsed);
        assert_eq!(" rest", remaining);
    }

    #[test]
    fn test_version_parser() {
        let (parsed, remaining) = ExtensionNameVersionPair::version_parser()
            .easy_parse("1.0,rest")
            .unwrap();
        assert_eq!("1.0", parsed);
        assert_eq!(",rest", remaining);
    }
}

impl ExtensionNameVersionPair {
    pub(crate) fn parser<Input>() -> impl Parser<Input, Output = Result<Self, ExtensionConfigError>>
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
                Self::try_new(&name, &version)
            })
    }
}

#[cfg(test)]
mod test_parser {
    use combine::EasyParser;

    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

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
    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

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
    use crate::extensions::name_version_pair::ExtensionNameVersionPair;

    #[test]
    fn test_parse_version_ok() {
        let (parsed, remaining) = ExtensionNameVersionPair::parse_version("1.0 rest").unwrap();
        assert_eq!("1.0", parsed);
        assert_eq!(" rest", remaining);
    }
}
