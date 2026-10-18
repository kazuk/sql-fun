use super::{SchemaCatalog, SchemaFileContext};
use crate::sem::{
    AlterOwner, AnalysisError, CreateFunction, CreateSchema, FullName, ObjectKind,
    OwnedObjectDefinition, OwnershipInfoRead, Role, RoleInfoRead, SequenceInfoRead, TableInfoRead,
    TypeInfoRead, ViewInfoRead,
};

impl RoleInfoRead for SchemaCatalog {
    fn role_by_name(&self, name: &str) -> Option<&Role> {
        if let Some(role) = self.ownership.role(name) {
            Some(role)
        } else {
            self.base.role_by_name(name)
        }
    }
}
impl OwnershipInfoRead for SchemaCatalog {
    fn schema_definition(&self, name: &str) -> Option<&CreateSchema> {
        if let Some(definition) = self.schemas.get(name) {
            Some(definition)
        } else {
            self.base.schema_definition(name)
        }
    }
    fn routine_definitions(&self, name: &FullName) -> Vec<CreateFunction> {
        let mut definitions = self.functions.definitions(name);
        for base in self.base.routine_definitions(name) {
            let identity = base.ownership_identity(name);
            if !CreateFunction::contains_identity(&definitions, name, &identity) {
                definitions.push(base);
            }
        }
        definitions
    }
    fn ownership_definitions(
        &self,
        kind: ObjectKind,
        name: &FullName,
    ) -> Vec<OwnedObjectDefinition> {
        let modeled = crate::sem::ownership::model_definitions(self, kind, name);
        let mut definitions = self.ownership.definitions(kind, name);
        if !modeled.is_empty() {
            // Richer DDL is authoritative; inventory records supply ownership only
            // for immutable terminal definitions such as builtin types.
            if definitions.is_empty() {
                if kind.code() == 50
                    && self
                        .get_type_impl(name)
                        .is_some_and(|definition| definition.is_builtin())
                {
                    let inherited = self.base.ownership_definitions(kind, name);
                    if !inherited.is_empty() {
                        return inherited;
                    }
                }
                return modeled;
            }
            for definition in modeled {
                if !OwnedObjectDefinition::contains_identity(&definitions, definition.identity()) {
                    definitions.push(definition);
                }
            }
            return definitions;
        }
        for base in self.base.ownership_definitions(kind, name) {
            if !OwnedObjectDefinition::contains_identity(&definitions, base.identity()) {
                definitions.push(base);
            }
        }
        definitions
    }
}
impl SchemaFileContext {
    pub(super) fn change_dependent_sequence_owners(
        &mut self,
        table: &crate::sem::TableName,
        owner: &crate::sem::OwnerSpecification,
    ) {
        for mut sequence in self.sequences_owned_by(table) {
            sequence.change_owner(owner);
            self.catalog.sequences.insert(sequence.name(), &sequence);
        }
    }

    pub(super) fn apply_ownership_change(
        mut self,
        alteration: &AlterOwner,
    ) -> Result<Self, AnalysisError> {
        let Some(identity) = alteration.applied_identity() else {
            return Ok(self);
        };
        let name = identity.name();
        let owner = alteration.owner();
        match identity.kind().code() {
            42 => {
                if let Some(mut definition) = self.get_table_impl(name).cloned() {
                    definition.change_owner(owner);
                    self.catalog.tables.insert_create_table(name, &definition);
                    self.change_dependent_sequence_owners(
                        &crate::sem::TableName::from(name.clone()),
                        owner,
                    );
                }
            }
            38 => {
                if let Some(mut definition) = self.get_sequence_impl(name).cloned() {
                    definition.change_owner(owner);
                    self.catalog.sequences.insert(name, &definition);
                }
            }
            24 | 52 => {
                if let Some(mut definition) = self.get_view_impl(name).cloned() {
                    definition.change_owner(owner);
                    self.catalog.views.insert(name, &definition);
                }
            }
            13 | 50 => {
                if let Some(mut definition) = self.get_type_impl(name).cloned() {
                    if definition.change_owner(owner) {
                        self.catalog.types.replace_definition(name, &definition);
                    } else {
                        self.catalog
                            .ownership
                            .insert(&OwnedObjectDefinition::new(identity, Some(owner)));
                    }
                } else {
                    self.catalog
                        .ownership
                        .insert(&OwnedObjectDefinition::new(identity, Some(owner)));
                }
            }
            37 => {
                if let Some(mut definition) = self.schema_definition(name.name().as_str()).cloned()
                {
                    definition.change_owner(owner);
                    self.catalog
                        .schemas
                        .insert(name.name().as_str(), definition);
                } else {
                    self.catalog
                        .ownership
                        .insert(&OwnedObjectDefinition::new(identity, Some(owner)));
                }
            }
            20 | 30 => {
                if let Some(mut definition) = self
                    .routine_definitions(name)
                    .into_iter()
                    .find(|definition| definition.ownership_identity(name) == *identity)
                {
                    definition.change_owner(owner);
                    self.catalog.functions.insert(name, &definition);
                } else {
                    self.catalog
                        .ownership
                        .insert(&OwnedObjectDefinition::new(identity, Some(owner)));
                }
            }
            _ => self
                .catalog
                .ownership
                .insert(&OwnedObjectDefinition::new(identity, Some(owner))),
        }
        Ok(self)
    }
}
