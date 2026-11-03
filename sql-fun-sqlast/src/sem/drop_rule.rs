use crate::{
    names::FullName,
    sem::{AnalysisError, DropDependencyBehavior, ParseContext, RuleIdentity, SemAst},
};

/// Resolved DROP RULE and the completed lookup outcome.
/// An IF EXISTS no-op retains the requested names and any resolved relation.
#[derive(Debug, Clone)]
pub struct DropRule {
    requested: FullName,
    requested_target: FullName,
    identity: Option<RuleIdentity>,
    found: bool,
    if_exists: bool,
    behavior: DropDependencyBehavior,
}
impl DropRule {
    pub(crate) fn new(
        requested: &FullName,
        requested_target: &FullName,
        identity: Option<&RuleIdentity>,
        found: bool,
        if_exists: bool,
        behavior: DropDependencyBehavior,
    ) -> Self {
        Self {
            requested: requested.clone(),
            requested_target: requested_target.clone(),
            identity: identity.cloned(),
            found,
            if_exists,
            behavior,
        }
    }
    /// Rule name supplied by the statement.
    pub const fn requested_name(&self) -> &FullName {
        &self.requested
    }
    /// ON relation name supplied by the statement.
    pub const fn requested_target(&self) -> &FullName {
        &self.requested_target
    }
    /// Relation-scoped identity, absent only if the ON relation was not found.
    pub const fn identity(&self) -> Option<&RuleIdentity> {
        self.identity.as_ref()
    }
    /// Whether the rule lookup found an effective definition.
    pub const fn found(&self) -> bool {
        self.found
    }
    /// Whether missing relations and rules are accepted as no-ops.
    pub const fn if_exists(&self) -> bool {
        self.if_exists
    }
    /// Requested dependency behavior.
    pub const fn behavior(&self) -> DropDependencyBehavior {
        self.behavior
    }
    pub(crate) fn apply<C: ParseContext>(&self, context: C) -> Result<C, AnalysisError> {
        if !self.found {
            return Ok(context);
        }
        let identity = self
            .identity
            .as_ref()
            .expect("found rule has resolved identity");
        let target = identity.target();
        context
            .cancel_rule(identity)?
            .dissociate_rule(target, identity)
    }
}
impl SemAst {
    /// Retain a resolved DROP RULE statement.
    pub fn drop_rule(statement: &DropRule) -> Self {
        Self::DropRule(statement.clone())
    }
    /// Inspect DROP RULE semantics and completed lookup outcome.
    pub const fn rule_drop(&self) -> Option<&DropRule> {
        if let Self::DropRule(statement) = self {
            Some(statement)
        } else {
            None
        }
    }
}
