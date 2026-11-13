//! Project lint findings and configuration interpretation warnings.
use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata, StringSpan};
use std::fmt::Write;

/// Structured target facts for a project lint diagnostic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
pub struct LintTarget {
    kind: String,
    name: Vec<String>,
    column: Option<String>,
    label: Option<String>,
}
impl LintTarget {
    /// Exact target components and optional column/label designation.
    #[must_use]
    pub fn new(kind: &str, name: &[String], column: Option<&str>, label: Option<&str>) -> Self {
        Self {
            kind: kind.to_owned(),
            name: name.to_vec(),
            column: column.map(str::to_owned),
            label: label.map(str::to_owned),
        }
    }
    fn description(&self) -> String {
        let mut target = format!("{} {:?}", self.kind, self.name);
        if let Some(column) = &self.column {
            write!(target, " column {column:?}").expect("write to string");
        }
        if let Some(label) = &self.label {
            write!(target, " label {label:?}").expect("write to string");
        }
        target
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
enum LintProblemKind {
    Deprecated,
    Configuration,
    Duplicate,
}
impl LintProblemKind {
    const fn code(&self) -> &'static str {
        match self {
            Self::Deprecated => "deprecated",
            Self::Configuration => "lint_configuration",
            Self::Duplicate => "duplicate_lint_target",
        }
    }
}

/// Structured warning produced by project semantic linting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
pub struct LintProblem {
    kind: LintProblemKind,
    target: LintTarget,
    reason: String,
    span: Option<StringSpan>,
}
impl LintProblem {
    /// Reference to a configured deprecated object.
    #[must_use]
    pub fn deprecated(target: &LintTarget) -> Self {
        Self::new(
            LintProblemKind::Deprecated,
            target,
            "reference to a deprecated schema object",
        )
    }
    /// Configuration cannot be applied to the selected semantic catalog.
    #[must_use]
    pub fn configuration(target: &LintTarget, reason: &str) -> Self {
        Self::new(LintProblemKind::Configuration, target, reason)
    }
    /// Multiple declarations select the same resolved lint target.
    #[must_use]
    pub fn duplicate(target: &LintTarget) -> Self {
        Self::new(
            LintProblemKind::Duplicate,
            target,
            "duplicate deprecated lint target",
        )
    }
    fn new(kind: LintProblemKind, target: &LintTarget, reason: &str) -> Self {
        Self {
            kind,
            target: target.clone(),
            reason: reason.to_owned(),
            span: None,
        }
    }
    /// Attach a reliable source span supplied by a semantic reference.
    #[must_use]
    pub const fn with_span(mut self, span: Option<StringSpan>) -> Self {
        self.span = span;
        self
    }
    /// Stable diagnostic classification.
    #[must_use]
    pub const fn code(&self) -> &str {
        self.kind.code()
    }
    /// Reliable semantic-reference location, if retained.
    #[must_use]
    pub const fn span(&self) -> Option<StringSpan> {
        self.span
    }
    /// Standard presentation for this warning.
    #[must_use]
    pub fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for LintProblem {
    fn diagnostic_message(&self) -> String {
        format!("{}: {}", self.reason, self.target.description())
    }
}

#[cfg(test)]
mod test_lint_diagnostics {
    use crate::analysis_problem::{
        AnalysisProblem, DiagnosticMessage, DiagnosticSeverity, LintProblem, LintTarget, StringSpan,
    };

    #[rstest::rstest]
    #[case::deprecated("deprecated")]
    #[case::configuration("lint_configuration")]
    #[case::duplicate("duplicate_lint_target")]
    fn structured_warnings_round_trip(#[case] code: &str) {
        let target = LintTarget::new(
            "table",
            &["public".to_owned(), "items".to_owned()],
            Some("legacy"),
            None,
        );
        let problem = match code {
            "deprecated" => LintProblem::deprecated(&target),
            "lint_configuration" => {
                LintProblem::configuration(&target, "column cannot be resolved")
            }
            _ => LintProblem::duplicate(&target),
        };
        let problem = AnalysisProblem::lint(&problem);
        let restored: AnalysisProblem =
            serde_json::from_str(&serde_json::to_string(&problem).unwrap()).unwrap();
        assert_eq!(restored.code(), code);
        assert_eq!(restored.severity(), DiagnosticSeverity::Warning);
        assert_eq!(restored.diagnostic_message(), problem.diagnostic_message());
        assert!(restored.statement_relative_span().is_none());
        assert!(restored.render_metadata().is_some());
    }
    #[test]
    fn retains_only_an_explicit_semantic_span() {
        let target = LintTarget::new("type", &["mood".to_owned()], None, None);
        let problem = AnalysisProblem::lint(
            &LintProblem::deprecated(&target).with_span(Some(StringSpan::new(4, 5))),
        );
        assert_eq!(
            problem.statement_relative_span(),
            Some(StringSpan::new(4, 5))
        );
    }
}
