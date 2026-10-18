use crate::sem::{AnalysisError, FullName, RuleIdentity};

/// Structured rule-definition or application failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleProblem {
    /// Missing or unsuitable target relation.
    Target(FullName),
    /// An existing definition conflicts with CREATE.
    Duplicate(RuleIdentity),
    /// A rule-specific legality constraint was violated.
    Definition(RuleDefinitionProblem),
}
impl RuleProblem {
    /// Rule-specific legality restriction, if this is a definition failure.
    pub fn definition_detail(&self) -> Option<&str> {
        if let Self::Definition(problem) = self {
            Some(problem.detail())
        } else {
            None
        }
    }
    pub(crate) fn target(target: &FullName) -> Self {
        Self::Target(target.clone())
    }
    pub(crate) fn duplicate(identity: &RuleIdentity) -> Self {
        Self::Duplicate(identity.clone())
    }
    pub(crate) fn definition(identity: &RuleIdentity, detail: &str) -> Self {
        Self::Definition(RuleDefinitionProblem::new(identity, detail))
    }
    pub(super) fn render_report(
        &self,
        handler: &::miette::GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let diagnostic = ::miette::MietteDiagnostic::new(format!("invalid CREATE RULE: {self:?}"))
            .with_code("rule_invalid_definition")
            .with_severity(::miette::Severity::Error);
        let report = ::miette::Report::new(diagnostic)
            .with_source_code(::miette::NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}

/// A rule identity and the semantic restriction it violates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDefinitionProblem {
    identity: RuleIdentity,
    detail: String,
}
impl RuleDefinitionProblem {
    fn new(identity: &RuleIdentity, detail: &str) -> Self {
        Self {
            identity: identity.clone(),
            detail: detail.to_owned(),
        }
    }
    /// Rule being analyzed.
    pub const fn identity(&self) -> &RuleIdentity {
        &self.identity
    }
    /// Violated semantic restriction.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
