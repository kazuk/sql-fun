//! Loads immutable lint definitions without interpreting their semantic applicability.

use std::{
    error::Error as StdError,
    path::{Path, PathBuf},
};

/// Immutable project lint definitions, independent of analysis-period state.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LintConfigurations {
    #[serde(default)]
    lint: LintItems,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct LintItems {
    #[serde(default)]
    deprecated: Vec<DeprecatedLintItem>,
}

impl LintConfigurations {
    /// Load an explicitly selected file. An existing empty file has no definitions.
    ///
    /// # Errors
    /// Reports missing or unreadable files, malformed TOML, and known fields that cannot decode.
    pub fn load_from(path: &Path) -> Result<Self, LintConfigurationError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| LintConfigurationError::new(path, error))?;
        toml::from_str(&text).map_err(|error| LintConfigurationError::new(path, error))
    }

    /// Deprecated rules in configured order, including repeated targets.
    #[must_use]
    pub fn deprecated(&self) -> &[DeprecatedLintItem] {
        &self.lint.deprecated
    }
}

#[cfg(test)]
mod test_lint_loading {
    use crate::{
        LintConfigurations,
        test_helpers::{TestProjectDir, parse_lint_fixture},
    };
    use std::{
        error::Error as StdError,
        io::{Error as IoError, ErrorKind},
    };

    #[rstest::rstest]
    #[case::empty("")]
    #[case::whitespace(" \n\t")]
    #[case::comment("# no definitions\n")]
    #[case::empty_lint("[lint]")]
    #[case::unknown_fields("future = true\n[lint]\nfuture = [1, 2]")]
    fn empty_configurations(#[case] text: &str) {
        assert!(parse_lint_fixture(text).unwrap().deprecated().is_empty());
    }

    #[test]
    fn missing_file_preserves_not_found_error() {
        let project = TestProjectDir::new().unwrap();
        let path = project.path().join("missing.toml");
        let error = LintConfigurations::load_from(&path).unwrap_err();
        let source = error.source().unwrap().downcast_ref::<IoError>().unwrap();
        assert_eq!(source.kind(), ErrorKind::NotFound);
        assert!(error.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn directory_is_a_read_error() {
        let project = TestProjectDir::new().unwrap();
        let error = LintConfigurations::load_from(project.path()).unwrap_err();
        assert!(error.source().unwrap().downcast_ref::<IoError>().is_some());
        assert!(
            error
                .to_string()
                .contains(&project.path().display().to_string())
        );
    }

    #[rstest::rstest]
    #[case::syntax("[[lint.deprecated]")]
    #[case::known_field_type(
        r#"[[lint.deprecated]]
kind = "table"
name = 1"#
    )]
    fn decoding_errors_include_file_and_cause(#[case] text: &str) {
        let message = parse_lint_fixture(text).unwrap_err().to_string();
        assert!(message.contains("lint.toml"));
        assert!(message.contains("TOML"));
    }

    #[test]
    fn invalid_utf8_is_an_error() {
        let project = TestProjectDir::new().unwrap();
        let path = project.path().join("lint.toml");
        std::fs::write(&path, [0xff]).unwrap();
        assert!(LintConfigurations::load_from(&path).is_err());
    }
}

/// A configured target; semantic constraints are applied by the lint consumer.
///
/// Names contain exact identifier components, not SQL identifier syntax.
/// Unknown TOML fields are ignored for compatibility across versions.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeprecatedLintItem {
    kind: String,
    name: Vec<String>,
    column: Option<String>,
    label: Option<String>,
    expect: Option<bool>,
}

impl DeprecatedLintItem {
    /// Target kind, interpreted by the semantic lint consumer.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Exact name components, preserving case, embedded dots, and qualification.
    #[must_use]
    pub fn name(&self) -> &[String] {
        &self.name
    }

    /// Optional column designation, preserved independently of target kind.
    #[must_use]
    pub fn column(&self) -> Option<&str> {
        self.column.as_deref()
    }

    /// Optional enum label, preserved independently of target kind.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Explicit expectation choice; only `Some(true)` requests an observation.
    #[must_use]
    pub const fn expect(&self) -> Option<bool> {
        self.expect
    }
}

#[cfg(test)]
mod test_deprecated_decoding {
    use crate::test_helpers::parse_lint_fixture;

    #[rstest::rstest]
    #[case::table("table")]
    #[case::view("view")]
    #[case::function("function")]
    #[case::type_target("type")]
    #[case::domain("domain")]
    #[case::enum_label("enum_label")]
    fn preserves_supported_fields_without_semantic_checks(#[case] kind: &str) {
        let text = format!(
            r#"
future = 42
[[lint.deprecated]]
kind = "{kind}"
name = ["Public.Schema", "MixedCase"]
column = "Column.Name"
label = "Label"
expect = true
future = {{ enabled = true }}
"#
        );
        let configuration = parse_lint_fixture(&text).unwrap();
        let rule = &configuration.deprecated()[0];
        assert_eq!(rule.kind(), kind);
        assert_eq!(rule.name(), ["Public.Schema", "MixedCase"]);
        assert_eq!(rule.column(), Some("Column.Name"));
        assert_eq!(rule.label(), Some("Label"));
        assert_eq!(rule.expect(), Some(true));
    }

    #[test]
    fn preserves_repeated_targets_and_omitted_fields() {
        let configuration = parse_lint_fixture(
            r#"
[[lint.deprecated]]
kind = "table"
name = ["users"]
[[lint.deprecated]]
kind = "table"
name = ["users"]
expect = false
"#,
        )
        .unwrap();
        assert_eq!(configuration.deprecated().len(), 2);
        let first = &configuration.deprecated()[0];
        assert_eq!(first.name(), ["users"]);
        assert_eq!(first.column(), None);
        assert_eq!(first.label(), None);
        assert_eq!(first.expect(), None);
        assert_eq!(configuration.deprecated()[1].expect(), Some(false));
    }
}

/// An I/O or decoding failure with the selected lint file's path.
#[derive(Debug, thiserror::Error)]
#[error("lint configuration {path}: {source}")]
pub struct LintConfigurationError {
    path: PathBuf,
    #[source]
    source: Box<dyn StdError + Send + Sync>,
}

impl LintConfigurationError {
    fn new(path: &Path, source: impl StdError + Send + Sync + 'static) -> Self {
        Self {
            path: path.to_path_buf(),
            source: Box::new(source),
        }
    }
}
