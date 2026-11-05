mod ast;

use crate::{
    names::FullName,
    sem::{AnalysisError, DropDependencyBehavior, ParseContext, SemAst, TriggerIdentity},
};

/// Resolved DROP TRIGGER and the completed lookup outcome.
/// An IF EXISTS no-op retains the requested names and any resolved relation.
#[derive(Debug, Clone)]
pub struct DropTrigger {
    requested: FullName,
    requested_target: Option<FullName>,
    target: Option<FullName>,
    identity: Option<TriggerIdentity>,
    found: bool,
    if_exists: bool,
    behavior: Option<DropDependencyBehavior>,
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
            requested_target: Some(requested_target.clone()),
            target: if let Some(identity) = identity {
                identity.relation().cloned()
            } else {
                None
            },
            identity: identity.cloned(),
            found,
            if_exists,
            behavior: Some(behavior),
        }
    }
    pub(crate) fn sqlite(
        requested: &FullName,
        identity: Option<&TriggerIdentity>,
        target: Option<&FullName>,
        if_exists: bool,
    ) -> Self {
        Self {
            requested: requested.clone(),
            requested_target: None,
            target: target.cloned(),
            identity: identity.cloned(),
            found: target.is_some(),
            if_exists,
            behavior: None,
        }
    }
    /// Resolved owning relation, including SQLite's target discovered from its named definition.
    pub const fn target(&self) -> Option<&FullName> {
        self.target.as_ref()
    }
    /// Trigger name supplied by the statement.
    pub const fn requested_name(&self) -> &FullName {
        &self.requested
    }
    /// ON relation supplied by PostgreSQL; absent in SQLite.
    pub const fn requested_target(&self) -> Option<&FullName> {
        self.requested_target.as_ref()
    }
    /// Resolved dialect-specific identity; absent if unqualified lookup found no object.
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
    /// Requested PostgreSQL dependency behavior; absent in SQLite.
    pub const fn behavior(&self) -> Option<DropDependencyBehavior> {
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
        let target = self
            .target
            .as_ref()
            .expect("found trigger has owning relation");
        context
            .cancel_trigger(identity)?
            .dissociate_trigger(target, identity.name())
    }
}

#[cfg(test)]
mod test_drop_masks_inherited_definition_and_recreation_replaces_the_tombstone {
    use crate::{
        context::{ParseContext, TriggerInfoRead},
        test_helpers::{drop_trigger as test_helpers, heavy::drop_trigger as heavy},
    };

    #[test]
    fn drop_masks_inherited_definition_and_recreation_replaces_the_tombstone()
    -> testresult::TestResult {
        let parent = heavy::catalog()?;
        let child = test_helpers::apply(
            test_helpers::layer(&parent),
            "DROP TRIGGER audit ON item_view",
        )?
        .into_context()
        .freeze();
        assert!(child.trigger(&test_helpers::identity()).is_none());
        assert!(
            test_helpers::layer(&child)
                .trigger(&test_helpers::identity())
                .is_none()
        );
        assert!(parent.trigger(&test_helpers::identity()).is_some());
        assert!(
            test_helpers::layer(&parent)
                .trigger(&test_helpers::identity())
                .is_some()
        );
        assert_eq!(test_helpers::active_count(&parent), 2);
        assert_eq!(test_helpers::active_count(&child), 1);
        let grandchild =
            test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
                .into_context()
                .freeze();
        assert!(grandchild.trigger(&test_helpers::identity()).is_some());
        assert_eq!(test_helpers::active_count(&grandchild), 2);
        assert!(child.trigger(&test_helpers::identity()).is_none());
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod test_drop_masks_inherited_trigger_and_recreation_preserves_other_links {
    use crate::{
        context::{ParseContext, TriggerInfoRead},
        names::FullName,
        test_helpers::{drop_trigger_sqlite as test_helpers, heavy::drop_trigger_sqlite as heavy},
    };

    #[test]
    fn drop_masks_inherited_trigger_and_recreation_preserves_other_links() -> testresult::TestResult
    {
        let parent = heavy::catalog()?;
        let pair = test_helpers::apply(test_helpers::layer(&parent), "DROP TRIGGER AUDIT")?;
        let drop = pair.ast().trigger_drop().unwrap();
        assert_eq!(drop.identity(), Some(&test_helpers::identity()));
        assert_eq!(
            drop.target(),
            Some(&FullName::with_schema("main", "item_view"))
        );
        assert!(drop.requested_target().is_none());
        assert!(drop.behavior().is_none());
        let child = pair.into_context().freeze();
        assert!(child.trigger(&test_helpers::identity()).is_none());
        assert!(
            test_helpers::layer(&child)
                .trigger(&test_helpers::identity())
                .is_none()
        );
        assert_eq!(test_helpers::active_count(&child), 1);
        assert_eq!(test_helpers::active_count(&parent), 2);
        assert!(
            test_helpers::layer(&parent)
                .trigger(&test_helpers::identity())
                .is_some()
        );
        let recreated =
            test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
                .into_context()
                .freeze();
        assert_eq!(test_helpers::active_count(&recreated), 2);
        assert!(recreated.trigger(&test_helpers::identity()).is_some());
        Ok(())
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
