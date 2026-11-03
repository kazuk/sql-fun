use crate::{
    names::FullName,
    sem::{AnalysisError, CreateTrigger, TriggerIdentity},
};

/// Read analyzed triggers and effective cancellations through context layers.
pub trait TriggerInfoRead {
    /// Whether effective lookup stopped at an explicit trigger cancellation.
    /// Missing definitions without a cancellation return false.
    fn trigger_is_cancelled(&self, _identity: &TriggerIdentity) -> bool {
        false
    }

    /// Look up an exact trigger identity in the effective context layer.
    fn trigger(&self, identity: &TriggerIdentity) -> Option<&CreateTrigger>;
}

/// Explicit definition and target-link operations selected by trigger DDL.
pub trait TriggerInfoWrite: Sized {
    /// Register a completed definition in this layer. Ownership is decided by DDL application.
    fn register_trigger(self, trigger: &CreateTrigger) -> Result<Self, AnalysisError>;
    /// Associate the supplied name with the exact target in this layer.
    fn associate_trigger(self, target: &FullName, name: &FullName) -> Result<Self, AnalysisError>;

    /// Mask a trigger identity in this layer, stopping inherited lookup.
    /// DDL separately selects the matching target-link update.
    fn cancel_trigger(self, identity: &TriggerIdentity) -> Result<Self, AnalysisError>;
    /// Remove an explicitly selected trigger reference from a current-layer target clone.
    fn dissociate_trigger(self, target: &FullName, name: &FullName) -> Result<Self, AnalysisError>;
}
