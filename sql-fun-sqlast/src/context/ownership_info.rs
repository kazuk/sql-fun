use crate::{
    names::FullName,
    sem::{ObjectKind, OwnedObjectDefinition, Role},
};

/// Exact-name role lookup supplied by the explicitly selected analysis environment.
pub trait RoleInfoRead {
    /// Explicitly registered semantic roles, independent of project configuration.
    /// Layered contexts use this to preserve registrations without inheriting a
    /// frozen catalog's historical configured inventory. Providers that include
    /// project-configured roles in `role_by_name` must override this method to
    /// exclude those entries; the default preserves existing semantic providers.
    fn registered_role_by_name(&self, name: &str) -> Option<&Role> {
        self.role_by_name(name)
    }

    /// Missing roles remain missing; implementations must not fabricate a role.
    fn role_by_name(&self, _name: &str) -> Option<&Role> {
        None
    }
}

/// Exact catalog reads used for ownership analysis (no search-path expansion here).
pub trait OwnershipInfoRead {
    /// Retained schema DDL by exact global schema name.
    fn schema_definition(&self, _name: &str) -> Option<&crate::sem::CreateSchema> {
        None
    }

    /// Retained routine DDL, one definition per exact input signature.
    fn routine_definitions(&self, _name: &FullName) -> Vec<crate::sem::CreateFunction> {
        Vec::new()
    }

    /// Existing definitions of the requested class at this exact name.
    /// ROUTINE includes functions and procedures. Return one item per overload.
    fn ownership_definitions(
        &self,
        _kind: ObjectKind,
        _name: &FullName,
    ) -> Vec<OwnedObjectDefinition> {
        Vec::new()
    }
}
