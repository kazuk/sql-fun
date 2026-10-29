//! Configuration and storage for catalog ownership.

use crate::sem::{FullName, ObjectKind, OwnedObjectDefinition, Role};

/// Roles derived from the current analysis metadata, independent of frozen configuration.
#[derive(Debug, Default)]
pub(crate) struct ConfiguredRoles(std::collections::HashMap<String, Role>);
impl From<&::sql_fun_core::SqlFunMetadata> for ConfiguredRoles {
    fn from(metadata: &::sql_fun_core::SqlFunMetadata) -> Self {
        let mut roles = Self::default();
        if let Some(inventory) = metadata.roles() {
            for role in inventory.definitions::<Role>() {
                roles.0.insert(role.name().to_owned(), role);
            }
        }
        roles
    }
}
impl ConfiguredRoles {
    pub(crate) fn role(&self, name: &str) -> Option<&Role> {
        self.0.get(name)
    }
}

/// Layer-local inventory for object classes without richer DDL representations.
#[derive(Debug, Default)]
pub(crate) struct OwnershipCatalog {
    roles: std::collections::HashMap<String, Role>,
    definitions: Vec<OwnedObjectDefinition>,
}
impl OwnershipCatalog {
    pub(crate) fn insert_role(&mut self, role: &Role) {
        self.roles.insert(role.name().to_owned(), role.clone());
    }
    pub(crate) fn role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }
    pub(crate) fn insert(&mut self, definition: &OwnedObjectDefinition) {
        self.definitions
            .retain(|existing| existing.identity() != definition.identity());
        self.definitions.push(definition.clone());
    }
    pub(crate) fn definitions(
        &self,
        kind: ObjectKind,
        name: &FullName,
    ) -> Vec<OwnedObjectDefinition> {
        self.definitions
            .iter()
            .filter(|definition| {
                kind.accepts_owner_kind(definition.identity().kind())
                    && definition.identity().name() == name
            })
            .cloned()
            .collect()
    }
}
