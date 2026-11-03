use super::{DebugName, DiagnosticMessage, DiagnosticMetadata, TriggerIdentity};
/// Structured trigger-definition validation failure.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum TriggerProblem {
    /// Missing or invalid target relation.
    Target(DebugName),
    /// Duplicate trigger in its dialect namespace.
    Duplicate(TriggerIdentity),
    /// Requested trigger does not exist in the resolved relation namespace.
    Missing(TriggerIdentity),
    /// No `SQLite` trigger matched an unbound requested name.
    MissingName(DebugName),
    /// Unknown event column.
    Column(DebugName, String),
    /// Missing or invalid zero-input trigger function.
    Function(DebugName),
    /// Incompatible trigger options.
    Options(DebugName, String),
}
impl TriggerProblem {
    /// Diagnostic target.
    #[must_use]
    pub fn target(name: impl Into<DebugName>) -> Self {
        Self::Target(name.into())
    }
    /// Diagnostic duplicate.
    #[must_use]
    pub fn duplicate(identity: impl Into<TriggerIdentity>) -> Self {
        Self::Duplicate(identity.into())
    }
    /// Diagnostic missing name.
    #[must_use]
    pub fn missing_name(name: impl Into<DebugName>) -> Self {
        Self::MissingName(name.into())
    }
    /// Diagnostic missing.
    #[must_use]
    pub fn missing(identity: impl Into<TriggerIdentity>) -> Self {
        Self::Missing(identity.into())
    }
    /// Diagnostic column.
    #[must_use]
    pub fn column(target: impl Into<DebugName>, column: &str) -> Self {
        Self::Column(target.into(), column.to_owned())
    }
    /// Diagnostic function.
    #[must_use]
    pub fn function(name: impl Into<DebugName>) -> Self {
        Self::Function(name.into())
    }
    /// Diagnostic options.
    #[must_use]
    pub fn options(target: impl Into<DebugName>, rule: &str) -> Self {
        Self::Options(target.into(), rule.to_owned())
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::TriggerProblem;

    #[rstest::rstest]
    #[case::missing(
        TriggerProblem::missing("public.changed on public.items"),
        "trigger public.changed on public.items does not exist"
    )]
    #[case::column(
        TriggerProblem::column("public.items", "missing"),
        "event column missing does not exist on trigger target public.items"
    )]
    #[case::options(
        TriggerProblem::options("public.changed", "transition relations require AFTER"),
        "trigger public.changed has incompatible options: transition relations require AFTER"
    )]
    fn metadata_explains_the_retained_failure(
        #[case] problem: TriggerProblem,
        #[case] expected: &str,
    ) {
        assert_eq!(problem.render_metadata().message(), expected);
    }
}
