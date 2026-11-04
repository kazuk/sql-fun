use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata, StringSpan};

/// Invocation syntax and candidate-set argument binding failures.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum CallArgumentProblem {
    /// A supplied argument name occurs more than once.
    DuplicateName(String, Option<StringSpan>),
    /// No candidate recognizes the supplied argument name.
    UnknownName(String),
    /// Every candidate requires this omitted input.
    MissingRequired(Option<String>),
    /// Every candidate assigns this formal both positionally and by name.
    AssignedTwice(String),
    /// A positional argument follows a named argument (zero-based call position).
    PositionalAfterNamed(usize, Option<StringSpan>),
}
impl CallArgumentProblem {
    /// Diagnostic duplicate name.
    #[must_use]
    pub fn duplicate_name(name: &str, span: &Option<StringSpan>) -> Self {
        Self::DuplicateName(name.to_owned(), *span)
    }
    /// Diagnostic positional after named.
    #[must_use]
    pub const fn positional_after_named(position: usize, span: &Option<StringSpan>) -> Self {
        Self::PositionalAfterNamed(position, *span)
    }
    /// Diagnostic unknown formal name.
    #[must_use]
    pub fn unknown_name(name: &str) -> Self {
        Self::UnknownName(name.to_owned())
    }
    /// Diagnostic omitted required input.
    #[must_use]
    pub fn missing_required(name: Option<&str>) -> Self {
        Self::MissingRequired(name.map(str::to_owned))
    }
    /// Diagnostic repeated formal assignment.
    #[must_use]
    pub fn assigned_twice(name: &str) -> Self {
        Self::AssignedTwice(name.to_owned())
    }
    /// Diagnostic statement relative span.
    pub(super) const fn statement_relative_span(&self) -> Option<StringSpan> {
        match self {
            Self::DuplicateName(_, span) | Self::PositionalAfterNamed(_, span) => *span,
            Self::UnknownName(_) | Self::MissingRequired(_) | Self::AssignedTwice(_) => None,
        }
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::UnknownName(..) => "call_argument_unknown_name",
            Self::MissingRequired(..) => "call_argument_missing_required",
            Self::AssignedTwice(..) => "call_argument_assigned_twice",
            Self::DuplicateName(..) => "call_argument_duplicate_name",
            Self::PositionalAfterNamed(..) => "call_argument_positional_after_named",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = self.diagnostic_message();
        let mut metadata = DiagnosticMetadata::from(message);
        if let Some(span) = self.statement_relative_span() {
            metadata = metadata.with_label("invalid call argument", span);
        }
        metadata
    }
}

#[cfg(test)]
mod test_diagnostics {
    use crate::analysis_problem::call_argument::{CallArgumentProblem, StringSpan};

    #[test]
    fn binding_diagnostics_preserve_structured_formal_identity() {
        for (problem, code, payload) in [
            (
                CallArgumentProblem::unknown_name("z"),
                "call_argument_unknown_name",
                serde_json::json!({"UnknownName": "z"}),
            ),
            (
                CallArgumentProblem::missing_required(Some("a")),
                "call_argument_missing_required",
                serde_json::json!({"MissingRequired": "a"}),
            ),
            (
                CallArgumentProblem::assigned_twice("a"),
                "call_argument_assigned_twice",
                serde_json::json!({"AssignedTwice": "a"}),
            ),
        ] {
            assert_eq!(problem.code(), code);
            assert_eq!(serde_json::to_value(&problem).unwrap(), payload);
            let restored: CallArgumentProblem = serde_json::from_value(payload).unwrap();
            assert_eq!(restored, problem);
        }
    }

    #[test]
    fn retains_argument_location_and_label() {
        let problem = CallArgumentProblem::duplicate_name("a", &Some(StringSpan::new(24, 1)));
        let metadata = problem.render_metadata();
        assert_eq!(problem.code(), "call_argument_duplicate_name");
        assert_eq!(
            metadata.message(),
            "argument name \"a\" used more than once"
        );
        assert_eq!(metadata.labels()[0].message(), "invalid call argument");
        assert_eq!(metadata.labels()[0].span(), StringSpan::new(24, 1));
    }
}
