use crate::sem::{
    AnalysisError, DropDependencyBehavior, FullName, ParseContext, SemAst, TriggerIdentity,
};

/// Resolved DROP TRIGGER and the completed lookup outcome.
/// An IF EXISTS no-op retains the requested names and any resolved relation.
#[derive(Debug, Clone)]
pub struct DropTrigger {
    requested: FullName,
    requested_target: FullName,
    identity: Option<TriggerIdentity>,
    found: bool,
    if_exists: bool,
    behavior: DropDependencyBehavior,
}
impl DropTrigger {
    pub(crate) fn new(
        requested: &FullName,
        requested_target: &FullName,
        identity: Option<&TriggerIdentity>,
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
    /// Trigger name supplied by the statement.
    pub const fn requested_name(&self) -> &FullName {
        &self.requested
    }
    /// ON relation name supplied by the statement.
    pub const fn requested_target(&self) -> &FullName {
        &self.requested_target
    }
    /// Relation-scoped identity, absent only if the ON relation was not found.
    pub const fn identity(&self) -> Option<&TriggerIdentity> {
        self.identity.as_ref()
    }
    /// Whether the trigger lookup found an effective definition.
    pub const fn found(&self) -> bool {
        self.found
    }
    /// Whether missing relations and triggers are accepted as no-ops.
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
            .expect("found trigger has resolved identity");
        let target = identity
            .relation()
            .expect("PostgreSQL trigger has owning relation");
        context
            .cancel_trigger(identity)?
            .dissociate_trigger(target, identity.name())
    }
}
impl SemAst {
    /// Retain a resolved DROP TRIGGER statement.
    pub fn drop_trigger(statement: &DropTrigger) -> Self {
        Self::DropTrigger(statement.clone())
    }
    /// Inspect DROP TRIGGER semantics and completed lookup outcome.
    pub const fn trigger_drop(&self) -> Option<&DropTrigger> {
        if let Self::DropTrigger(statement) = self {
            Some(statement)
        } else {
            None
        }
    }
}
