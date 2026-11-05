use crate::{
    ast::ObjectKind,
    context::{
        OwnershipInfoRead, RoleInfoRead, SequenceInfoRead, TableInfoRead, TypeInfoRead,
        ViewInfoRead,
        schema_file_context::{SchemaCatalog, SchemaFileContext},
    },
    names::{FullName, TableName},
    sem::{
        AlterOwner, AnalysisError, CreateFunction, CreateSchema, OwnedObjectDefinition,
        OwnerSpecification, OwnershipIdentity, Role, ownership::model_definitions,
    },
};

impl RoleInfoRead for SchemaCatalog {
    fn role_by_name(&self, name: &str) -> Option<&Role> {
        if let Some(role) = self.registered_role_by_name(name) {
            Some(role)
        } else {
            self.configured_roles.role(name)
        }
    }
    fn registered_role_by_name(&self, name: &str) -> Option<&Role> {
        if let Some(role) = self.ownership.role(name) {
            Some(role)
        } else {
            self.base.registered_role_by_name(name)
        }
    }
}

#[cfg(test)]
mod test_roles_are_retained_in_the_selected_catalog {
    use crate::{
        context::{ParseContext, RoleInfoRead},
        sem::Role,
        test_helpers::ownership::ownership_context,
    };

    #[test]
    fn roles_are_retained_in_the_selected_catalog() {
        let context = ownership_context();
        assert_eq!(context.role_by_name("alice").map(Role::name), Some("alice"));
        assert!(context.role_by_name("ALICE").is_none());
        let frozen = context.freeze();
        assert_eq!(frozen.role_by_name("alice").map(Role::name), Some("alice"));
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
        if self.replaced_routines.contains(name) {
            return definitions;
        }
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
        if (kind == ObjectKind::TABLE && self.renamed_tables.contains(name))
            || (kind == ObjectKind::TYPE && self.renamed_types.contains(name))
        {
            return Vec::new();
        }
        let modeled = model_definitions(self, kind, name);
        let mut definitions = self.ownership.definitions(kind, name);
        if !modeled.is_empty() {
            // Current-layer inventory overrides precede modeled/base snapshots.
            // Mutable DDL is updated directly; immutable definitions retain only
            // an ownership override, which must also survive freezing.
            if definitions.is_empty() {
                if kind == ObjectKind::TYPE
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

#[cfg(test)]
mod test_ownership_only_override_wins_over_immutable_base_definition {
    use crate::{
        ast::ObjectKind,
        context::{OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::Role,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[rstest::rstest]
    #[case::unknown_base_owner(false)]
    #[case::known_base_owner(true)]
    fn ownership_only_override_wins_over_immutable_base_definition(
        #[case] known_base_owner: bool,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        if known_base_owner {
            context = apply_ownership_sql(context, "ALTER TYPE pg_catalog.int4 OWNER TO alice")?
                .into_context();
        }
        let name = FullName::with_schema("pg_catalog", "int4");
        let before = context.ownership_definitions(ObjectKind::TYPE, &name);
        let base = context.freeze();
        let child = schema_layer(std::path::Path::new("child.sql"), &base);
        let result = apply_ownership_sql(child, "ALTER TYPE pg_catalog.int4 OWNER TO bob")?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::TYPE, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        assert_eq!(base.ownership_definitions(ObjectKind::TYPE, &name), before);
        let child_base = result.into_context().freeze();
        let grandchild = schema_layer(std::path::Path::new("grandchild.sql"), &child_base);
        assert_eq!(
            grandchild.ownership_definitions(ObjectKind::TYPE, &name),
            definitions
        );
        let changed = apply_ownership_sql(grandchild, "ALTER TYPE pg_catalog.int4 OWNER TO alice")?;
        assert_eq!(
            changed
                .context()
                .ownership_definitions(ObjectKind::TYPE, &name)[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        assert_eq!(
            child_base.ownership_definitions(ObjectKind::TYPE, &name),
            definitions
        );
        Ok(())
    }
}
impl SchemaFileContext {
    pub(super) fn change_dependent_sequence_owners(
        &mut self,
        table: &TableName,
        owner: &OwnerSpecification,
    ) {
        for mut sequence in self.sequences_owned_by(table) {
            sequence.change_owner(owner);
            self.catalog.store_sequence(sequence.name(), &sequence);
        }
    }
}

#[cfg(test)]
mod test_dependent_sequence_owners_follow_table_changes_across_layers {
    use crate::{
        ast::ObjectKind,
        context::{MutSourcePathWithSpan, OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::Role,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer, source_span,
        },
    };

    #[test]
    fn dependent_sequence_owners_follow_table_changes_across_layers() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let base = context.freeze();
        let mut context = schema_layer(std::path::Path::new("next.sql"), &base);
        MutSourcePathWithSpan::set_source_path_with_span(&mut context, &source_span());
        let result = apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO bob")?;
        let name = FullName::with_schema("public", "s");
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &name);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        assert!(
            base.ownership_definitions(ObjectKind::from(38), &name)[0]
                .owner()
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn removed_sequence_dependency_does_not_leave_stale_owner_cascades() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let result = apply_ownership_sql(context, "ALTER SEQUENCE public.s OWNED BY NONE")?;
        let context = result.into_context();
        let result = apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO bob")?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &FullName::with_schema("public", "s"));
        assert!(definitions[0].owner().is_none());
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing_role("ALTER TABLE public.t OWNER TO missing")]
    #[case::unrelated_change("ALTER TABLE public.t ALTER COLUMN id SET DEFAULT 1")]
    fn only_a_successful_owner_command_cascades_to_sequences(
        #[case] alteration: &str,
    ) -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO alice")?.into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let result = apply_ownership_sql(context, alteration)?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &FullName::with_schema("public", "s"));
        assert!(definitions[0].owner().is_none());
        Ok(())
    }
}

impl SchemaFileContext {
    pub(super) fn apply_ownership_change(
        mut self,
        alteration: &AlterOwner,
    ) -> Result<Self, AnalysisError> {
        let Some(identity) = alteration.applied_identity() else {
            return Ok(self);
        };
        let owner = alteration.owner();
        match identity.kind() {
            ObjectKind::TABLE => self.change_table_owner(identity, owner),
            ObjectKind::SEQUENCE => self.change_sequence_owner(identity, owner),
            ObjectKind::MATERIALIZED_VIEW | ObjectKind::VIEW => {
                self.change_view_owner(identity, owner)?
            }
            ObjectKind::DOMAIN | ObjectKind::TYPE => self.change_type_owner(identity, owner),
            ObjectKind::SCHEMA => self.change_schema_owner(identity, owner),
            ObjectKind::FUNCTION | ObjectKind::PROCEDURE => {
                self.change_routine_owner(identity, owner)
            }
            _ => self.retain_owner_override(identity, owner),
        }
        Ok(self)
    }
    fn change_table_owner(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        let name = identity.name();
        if let Some(mut definition) = self.get_table_impl(name).cloned() {
            definition.change_owner(owner);
            self.catalog.store_table(name, &definition);
            self.change_dependent_sequence_owners(&TableName::from(name.clone()), owner);
        }
    }

    fn change_sequence_owner(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        let name = identity.name();
        if let Some(mut definition) = self.get_sequence_impl(name).cloned() {
            definition.change_owner(owner);
            self.catalog.store_sequence(name, &definition);
        }
    }

    fn change_view_owner(
        &mut self,
        identity: &OwnershipIdentity,
        owner: &OwnerSpecification,
    ) -> Result<(), AnalysisError> {
        let name = identity.name();
        if let Some(mut definition) = self.get_view_impl(name).cloned() {
            definition.change_owner(owner);
            self.catalog.store_view(name, &definition)?;
        }
        Ok(())
    }

    fn change_type_owner(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        let name = identity.name();
        if let Some(mut definition) = self.get_type_impl(name).cloned() {
            if definition.change_owner(owner) {
                self.catalog.types.replace_definition(name, &definition);
            } else {
                self.retain_owner_override(identity, owner);
            }
        } else {
            self.retain_owner_override(identity, owner);
        }
    }
}

#[cfg(test)]
mod test_ownership_preserves_shared_type_identity_and_serialization {
    use crate::{
        context::TypeInfoRead,
        names::FullName,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::domain("CREATE DOMAIN public.d AS integer", "DOMAIN", "d")]
    #[case::composite("CREATE TYPE public.d AS (id integer)", "TYPE", "d")]
    #[case::enum_type("CREATE TYPE public.d AS ENUM ('x')", "TYPE", "d")]
    fn ownership_preserves_shared_type_identity_and_serialization(
        #[case] create: &str,
        #[case] kind: &str,
        #[case] name: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let name = FullName::with_schema("public", name);
        let previous = context.get_type_impl(&name).unwrap().clone();
        let serialized = ::serde_json::to_value(&previous)?;
        let result =
            apply_ownership_sql(context, &format!("ALTER {kind} public.d OWNER TO alice"))?;
        let changed = result.context().get_type_impl(&name).unwrap();
        assert!(changed.owner().is_some());
        assert_eq!(changed, &previous);
        assert_eq!(::serde_json::to_value(changed)?, serialized);
        Ok(())
    }
}

impl SchemaFileContext {
    fn change_schema_owner(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        let name = identity.name();
        if let Some(mut definition) = self.schema_definition(name.name().as_str()).cloned() {
            definition.change_owner(owner);
            self.catalog
                .schemas
                .insert(name.name().as_str(), definition);
        } else {
            self.retain_owner_override(identity, owner);
        }
    }

    fn change_routine_owner(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        let name = identity.name();
        if let Some(mut definition) = self
            .routine_definitions(name)
            .into_iter()
            .find(|definition| definition.ownership_identity(name) == *identity)
        {
            definition.change_owner(owner);
            self.catalog.functions.insert(name, &definition);
        } else {
            self.retain_owner_override(identity, owner);
        }
    }
}

#[cfg(test)]
mod test_exact_overload_changes_survive_freezing_without_changing_base_or_siblings {
    use crate::{
        ast::ObjectKind,
        context::{BaseParseContext, OwnershipInfoRead, ParseContext},
        names::FullName,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[test]
    fn exact_overload_changes_survive_freezing_without_changing_base_or_siblings()
    -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let base = context.freeze();
        let next = schema_layer(std::path::Path::new("next.sql"), &base);
        let result = apply_ownership_sql(next, "ALTER FUNCTION public.f(integer) OWNER TO alice")?;
        let name = FullName::with_schema("public", "f");
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(20), &name);
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(&definitions),
            1
        );
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(
                &base.ownership_definitions(ObjectKind::from(20), &name)
            ),
            0
        );
        assert_eq!(
            result.context().get_function_by_name(&name).unwrap().len(),
            2
        );
        let frozen = result.into_context().freeze();
        assert_eq!(
            frozen.ownership_definitions(ObjectKind::from(20), &name),
            definitions
        );
        Ok(())
    }

    #[test]
    fn quoted_function_names_cannot_collide_with_schema_separators() -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.\"a.b\"() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION \"public.a\".b() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let result = apply_ownership_sql(context, "ALTER FUNCTION public.\"a.b\"() OWNER TO bob")?;
        let changed = result
            .context()
            .routine_definitions(&FullName::with_schema("public", "a.b"));
        let untouched = result
            .context()
            .routine_definitions(&FullName::with_schema("public.a", "b"));
        assert_eq!(changed.len(), 1);
        assert!(changed[0].owner().is_some());
        assert_eq!(untouched.len(), 1);
        assert!(untouched[0].owner().is_none());
        Ok(())
    }
}

impl SchemaFileContext {
    fn retain_owner_override(&mut self, identity: &OwnershipIdentity, owner: &OwnerSpecification) {
        self.catalog
            .ownership
            .insert(&OwnedObjectDefinition::new(identity, Some(owner)));
    }
}
