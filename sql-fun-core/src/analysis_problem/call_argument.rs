use super::{DiagnosticMetadata, StringSpan};

/// Invocation-shape violations decidable without a function signature.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum CallArgumentProblem {
    /// A supplied argument name occurs more than once.
    DuplicateName(String, Option<StringSpan>),
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
    /// Diagnostic statement relative span.
    pub(super) const fn statement_relative_span(&self) -> Option<StringSpan> {
        match self {
            Self::DuplicateName(_, span) | Self::PositionalAfterNamed(_, span) => *span,
        }
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateName(..) => "call_argument_duplicate_name",
            Self::PositionalAfterNamed(..) => "call_argument_positional_after_named",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
            Self::DuplicateName(name, _) => format!("argument name \"{name}\" used more than once"),
            Self::PositionalAfterNamed(position, _) => format!(
                "positional argument at call position {position} cannot follow named argument"
            ),
        };

        let mut metadata = DiagnosticMetadata::from(message);
        if let Some(span) = self.statement_relative_span() {
            metadata = metadata.with_label("invalid call argument", span);
        }
        metadata
    }
}

#[cfg(test)]
mod test_diagnostics {
    use super::{CallArgumentProblem, StringSpan};

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
