use std::collections::HashMap;

use crate::{
    names::FullName,
    sem::{CascadeRemoval, OperatorIdentity},
};

#[derive(Clone, Debug, Default)]
pub(super) struct OperatorRemovalRegistry {
    by_signature: HashMap<OperatorIdentity, Vec<(FullName, Box<dyn CascadeRemoval>)>>,
}

impl OperatorRemovalRegistry {
    pub(super) fn replace(
        &mut self,
        name: &FullName,
        previous: &[OperatorIdentity],
        current: &[OperatorIdentity],
        removal: &dyn CascadeRemoval,
    ) {
        for identity in previous {
            if let Some(entries) = self.by_signature.get_mut(identity) {
                entries.retain(|(owner, _)| owner != name);
                if entries.is_empty() {
                    self.by_signature.remove(identity);
                }
            }
        }
        for identity in current {
            self.by_signature
                .entry(identity.clone())
                .or_default()
                .push((name.clone(), removal.clone_box()));
        }
    }

    pub(super) fn for_operator(
        &self,
        identity: &OperatorIdentity,
    ) -> Vec<(FullName, Box<dyn CascadeRemoval>)> {
        self.by_signature.get(identity).cloned().unwrap_or_default()
    }
}
