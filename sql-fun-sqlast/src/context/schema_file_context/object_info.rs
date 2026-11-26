use crate::context::schema_file_context::view_info::ViewDependencies;

use crate::{
    StringSpan,
    ast::{Dialect, ObjectKind},
    context::{
        BaseParseContext, ExtensionInfoRead, ObjectInfoRead,
        schema_file_context::{SchemaCatalog, SchemaFileContext},
    },
    names::FullName,
    sem::{
        AnalysisError, CreateCompositeType, CreateIndex, CreateSequence, CreateTable,
        ExtensionMembershipChange, Kind, OwnershipIdentity, ViewLike,
    },
};

impl ObjectInfoRead for SchemaCatalog {
    fn relation_names(&self) -> Vec<FullName> {
        let mut names = self.base.relation_names();
        names.retain(|name| !self.renamed_relations.contains(name));
        self.tables.append_names(&mut names);
        self.views.append_names(&mut names);
        names.retain(|name| !self.renamed_relations.contains(name));
        names.retain(|name| !self.view_tombstones.contains_key(name));
        names
    }

    fn object_kind(&self, name: &FullName) -> Option<Kind> {
        let name = self.relation_identity(name);
        if self.renamed_relations.contains(&name) || self.view_tombstones.contains_key(&name) {
            return None;
        }
        if let Some(kind) = self.objects.get(&name.to_string()) {
            Some(*kind)
        } else {
            self.base.object_kind(&name)
        }
    }
}
impl ExtensionInfoRead for SchemaCatalog {
    fn extension_exists(&self, name: &str) -> bool {
        self.installed_extensions.contains_key(name)
            || self.extensions.contains(name)
            || self.base.extension_exists(name)
    }

    fn extension_schema(&self, name: &str) -> Option<&str> {
        match self.installed_extensions.get(name) {
            Some(schema) => schema.as_deref(),
            None => match self.extensions.get(name) {
                Some(extension) => extension.schema(),
                None => self.base.extension_schema(name),
            },
        }
    }

    fn extension_membership_history(
        &self,
        member: &OwnershipIdentity,
    ) -> &[ExtensionMembershipChange] {
        if let Some(history) = self.extension_memberships.get(member) {
            history
        } else {
            self.base.extension_membership_history(member)
        }
    }
}
impl SchemaCatalog {
    // SQLite folds ASCII identifiers even when quoted. Canonical Display still
    // encodes component boundaries after dialect-specific identity normalization.
    pub(super) fn relation_identity(&self, name: &FullName) -> FullName {
        if Dialect::from(self.metadata()) == Dialect::sqlite() {
            name.sqlite_identity()
        } else {
            name.clone()
        }
    }
    pub(super) fn store_table(&mut self, name: &FullName, definition: &CreateTable) {
        let name = self.relation_identity(name);
        self.renamed_relations.remove(&name);
        self.renamed_tables.remove(&name);
        self.renamed_types.remove(&name);
        self.view_tombstones.remove(&name);
        self.tables.insert_create_table(&name, definition);
        self.objects.insert(&name.to_string(), Kind::table());
    }
    pub(super) fn store_view(
        &mut self,
        name: &FullName,
        definition: &ViewLike,
    ) -> Result<(), AnalysisError> {
        let name = self.relation_identity(name);
        let dependencies = if Dialect::from(self.metadata()) == Dialect::postgres() {
            ViewDependencies::collect(definition)?
        } else {
            ViewDependencies::default()
        };
        let row_type = if Dialect::from(self.metadata()) == Dialect::postgres() {
            Some(CreateCompositeType::new(
                &name,
                &definition.result_columns().composite_field_collection()?,
            ))
        } else {
            None
        };
        self.renamed_relations.remove(&name);
        self.renamed_types.remove(&name);
        self.view_tombstones.remove(&name);
        self.register_view(&name, definition, dependencies);
        if let Some(row_type) = row_type {
            self.types.insert_create_composite_type(&name, &row_type);
        }
        let kind = if definition.is_materialized() {
            Kind::materialized_view()
        } else {
            Kind::view()
        };
        self.objects.insert(&name.to_string(), kind);
        Ok(())
    }
    pub(super) fn store_sequence(&mut self, name: &FullName, definition: &CreateSequence) {
        self.renamed_relations.remove(name);
        self.view_tombstones.remove(name);
        self.sequences.insert(name, definition);
        self.objects.insert(&name.to_string(), Kind::sequence());
    }
    pub(super) fn store_index(&mut self, definition: &CreateIndex) {
        self.renamed_relations.remove(definition.name());
        self.view_tombstones.remove(definition.name());
        self.indexes
            .insert(&definition.name().to_string(), definition.clone());
        self.objects
            .insert(&definition.name().to_string(), Kind::index());
    }
}
impl SchemaFileContext {
    pub(super) const fn creation_span(&self) -> StringSpan {
        self.statement_span
    }
}

impl SchemaCatalog {
    pub(super) fn rename_extension_member(
        &mut self,
        kind: ObjectKind,
        from: &FullName,
        to: &FullName,
    ) {
        let from = OwnershipIdentity::named(kind, from);
        let to = OwnershipIdentity::named(kind, to);
        let history = self.extension_membership_history(&from).to_vec();
        if !history.is_empty() || !self.extension_membership_history(&to).is_empty() {
            // An empty local history masks the parent entry at the retired identity.
            self.extension_memberships.insert(from, Vec::new());
            self.extension_memberships.insert(to, history);
        }
    }

    pub(super) fn apply_extension_membership(&mut self, change: &ExtensionMembershipChange) {
        let Some(member) = change.member().filter(|_| change.applicable()) else {
            return;
        };
        self.extension_memberships
            .entry(member.clone())
            .or_insert_with(|| self.base.extension_membership_history(member).to_vec())
            .push(change.clone());
    }
}

impl SchemaFileContext {
    pub(crate) fn enter_extension_script(&mut self, name: &str, schema: Option<&str>) {
        self.catalog
            .installed_extensions
            .insert(name.to_owned(), schema.map(str::to_owned));
        self.active_extension = Some(name.to_owned());
    }

    pub(super) fn record_named_extension_member(&mut self, kind: ObjectKind, name: &FullName) {
        if self.active_extension.is_some() {
            self.record_extension_member(&OwnershipIdentity::named(kind, name));
        }
    }

    pub(super) fn record_extension_member(&mut self, member: &OwnershipIdentity) {
        if let Some(extension) = &self.active_extension {
            let change =
                ExtensionMembershipChange::created(extension, member, self.catalog.source.as_ref());
            self.catalog.apply_extension_membership(&change);
        }
    }
}

#[cfg(test)]
mod test_extension_membership_history {
    use std::path::Path;

    use crate::{
        StringSpan,
        ast::ObjectKind,
        context::{ExtensionInfoRead, MutSourcePathWithSpan, OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{OwnershipIdentity, PgBuiltInType, SourcePathWithSpan},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[test]
    fn declared_operator_overloads_retain_extension_membership() -> testresult::TestResult {
        let mut context = ownership_context();
        context.enter_extension_script("operator_probe", Some("public"));
        for sql in [
            "CREATE FUNCTION identity(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
            "CREATE FUNCTION identity(integer, integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
            "CREATE OPERATOR %% (RIGHTARG=integer, FUNCTION=identity)",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=identity, COMMUTATOR='@#@#')",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty());
        let catalog = context.freeze();
        let name = FullName::with_schema("public", "%%");
        let operand = PgBuiltInType::int4();
        for left in [None, Some(operand.clone())] {
            let member = OwnershipIdentity::with_arguments(
                ObjectKind::OPERATOR,
                &name,
                &[left, Some(operand.clone())],
            );
            assert_eq!(
                catalog.extension_membership(&member),
                Some("operator_probe")
            );
            assert_eq!(catalog.extension_membership_history(&member).len(), 1);
        }
        let shell = OwnershipIdentity::with_arguments(
            ObjectKind::OPERATOR,
            &FullName::with_schema("public", "@#@#"),
            &[Some(operand.clone()), Some(operand)],
        );
        assert!(catalog.extension_membership_history(&shell).is_empty());
        Ok(())
    }

    #[test]
    fn update_masks_the_parent_membership_and_retains_the_member() -> testresult::TestResult {
        let mut install = ownership_context();
        install.enter_extension_script("pg_stat_statements", Some("public"));
        let span = StringSpan::new(6, 15);
        let create = SourcePathWithSpan::new(Path::new("pg_stat_statements--1.4.sql"), &span);
        install.set_source_path_with_span(&create);
        let install = apply_ownership_sql(
            install,
            "CREATE FUNCTION pg_stat_statements_reset() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context()
        .freeze();
        let name = FullName::with_schema("public", "pg_stat_statements_reset");
        let member = OwnershipIdentity::with_arguments(ObjectKind::FUNCTION, &name, &[]);
        let mut update = schema_layer(Path::new("pg_stat_statements--1.6--1.7.sql"), &install);
        let drop = SourcePathWithSpan::new(Path::new("pg_stat_statements--1.6--1.7.sql"), &span);
        update.set_source_path_with_span(&drop);
        let update = apply_ownership_sql(
            update,
            "ALTER EXTENSION pg_stat_statements DROP FUNCTION pg_stat_statements_reset()",
        )?
        .into_context()
        .freeze();
        assert_eq!(update.extension_membership(&member), None);
        assert_eq!(
            install.extension_membership(&member),
            Some("pg_stat_statements")
        );
        assert!(!update.routine_definitions(&name).is_empty());
        let history = update.extension_membership_history(&member);
        assert_eq!(history.len(), 2);
        assert!(history[0].is_add());
        assert_eq!(
            history[0].source().unwrap().source_path(),
            create.source_path()
        );
        assert!(!history[1].is_add());
        assert_eq!(
            history[1].source().unwrap().source_path(),
            drop.source_path()
        );
        let child = schema_layer(Path::new("repeat.sql"), &update);
        let rejected = apply_ownership_sql(
            child,
            "ALTER EXTENSION pg_stat_statements DROP FUNCTION pg_stat_statements_reset()",
        )?;
        assert_eq!(rejected.context().problems().len(), 1);
        assert_eq!(
            rejected
                .context()
                .extension_membership_history(&member)
                .len(),
            2
        );
        let child = apply_ownership_sql(
            rejected.into_context(),
            "ALTER EXTENSION pg_stat_statements ADD FUNCTION pg_stat_statements_reset()",
        )?
        .into_context();
        assert_eq!(
            child.extension_membership(&member),
            Some("pg_stat_statements")
        );
        assert_eq!(child.extension_membership_history(&member).len(), 3);
        assert_eq!(update.extension_membership(&member), None);
        Ok(())
    }
}

#[cfg(test)]
mod test_renamed_extension_membership {
    use std::path::Path;

    use crate::{
        ast::ObjectKind,
        context::{ExtensionInfoRead, MutSourcePathWithSpan, ParseContext},
        names::FullName,
        sem::{OwnershipIdentity, SemAst},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer, source_span,
        },
    };

    #[test]
    fn rename_moves_inherited_membership_and_preserves_history() -> testresult::TestResult {
        let mut context = ownership_context();
        for sql in [
            "CREATE EXTENSION ext",
            "CREATE TABLE public.t (id integer)",
            "ALTER EXTENSION ext ADD TABLE public.t",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        let parent = context.freeze();
        let old =
            OwnershipIdentity::named(ObjectKind::TABLE, &FullName::with_schema("public", "t"));
        let new =
            OwnershipIdentity::named(ObjectKind::TABLE, &FullName::with_schema("public", "u"));
        let mut child = schema_layer(Path::new("rename.sql"), &parent);
        child.set_source_path_with_span(&source_span());
        let renamed =
            apply_ownership_sql(child, "ALTER TABLE public.t RENAME TO u")?.into_context();
        assert_eq!(renamed.extension_membership(&old), None);
        assert_eq!(renamed.extension_membership(&new), Some("ext"));
        assert_eq!(parent.extension_membership(&old), Some("ext"));
        assert_eq!(parent.extension_membership(&new), None);
        assert_eq!(
            renamed.extension_membership_history(&new)[0].member(),
            Some(&old)
        );
        let detached = apply_ownership_sql(renamed, "ALTER EXTENSION ext DROP TABLE public.u")?;
        let SemAst::ExtensionMembership(change) = detached.ast() else {
            panic!("expected analyzed membership");
        };
        assert!(change.applicable());
        assert_eq!(detached.context().extension_membership(&new), None);
        assert_eq!(
            detached.context().extension_membership_history(&new).len(),
            2
        );
        Ok(())
    }
}
