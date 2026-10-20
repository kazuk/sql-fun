use super::{DebugName, DiagnosticMetadata, RuleIdentity};

/// Structured rule-definition or application failure.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum RuleProblem {
    /// Missing or unsuitable target relation.
    Target(DebugName),
    /// An existing definition conflicts with CREATE.
    Duplicate(RuleIdentity),
    /// No effective rule exists under this relation-scoped identity.
    Missing(RuleIdentity),
    /// A rule-specific legality constraint was violated.
    Definition(RuleDefinitionProblem),
}
impl RuleProblem {
    /// Rule-specific legality restriction, if this is a definition failure.
    #[must_use]
    pub fn definition_detail(&self) -> Option<&str> {
        if let Self::Definition(problem) = self {
            Some(problem.detail())
        } else {
            None
        }
    }
    /// Diagnostic target.
    #[must_use]
    pub fn target(target: impl Into<DebugName>) -> Self {
        Self::Target(target.into())
    }
    /// Diagnostic missing.
    #[must_use]
    pub fn missing(identity: impl Into<RuleIdentity>) -> Self {
        Self::Missing(identity.into())
    }
    /// Diagnostic duplicate.
    #[must_use]
    pub fn duplicate(identity: impl Into<RuleIdentity>) -> Self {
        Self::Duplicate(identity.into())
    }
    /// Diagnostic definition.
    #[must_use]
    pub fn definition(identity: impl Into<RuleIdentity>, detail: &str) -> Self {
        Self::Definition(RuleDefinitionProblem::new(identity, detail))
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(format!("invalid rule statement: {self:?}"))
    }
}

/// A rule identity and the semantic restriction it violates.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct RuleDefinitionProblem {
    identity: RuleIdentity,
    detail: String,
}
impl RuleDefinitionProblem {
    fn new(identity: impl Into<RuleIdentity>, detail: &str) -> Self {
        Self {
            identity: identity.into(),
            detail: detail.to_owned(),
        }
    }
    /// Rule being analyzed.
    #[must_use]
    pub const fn identity(&self) -> &RuleIdentity {
        &self.identity
    }
    /// Violated semantic restriction.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
