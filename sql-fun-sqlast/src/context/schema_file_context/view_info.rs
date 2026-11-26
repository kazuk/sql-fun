use std::{collections::HashMap, sync::Arc};

use crate::{
    TrieMap,
    ast::ObjectKind,
    context::{
        ExtensionInfoRead, ObjectInfoRead, ParseContext, RuleInfoWrite, TriggerInfoWrite,
        ViewDependents, ViewInfoRead, ViewInfoWrite,
        schema_file_context::{SchemaCatalog, SchemaFileContext},
    },
    names::{FullName, OperatorName},
    sem::{
        AlterView, AnalysisError, AnalysisProblem, CascadeRemoval, CreateTableAs, CreateView,
        DropView, ExtensionMembershipChange, Kind, OperatorIdentity, OwnershipIdentity,
        TriggerIdentity, TypeReference, ViewLike, ViewTombstone, problem::CreationProblem,
        search_path::expand_relation_search_path, traversal::SemanticVisitor,
    },
};

#[derive(Clone, Debug, Default)]
pub struct ViewInformationCollection {
    definitions: TrieMap<ViewLike>,
    names: Vec<FullName>,
    dependencies: HashMap<FullName, ViewDependencies>,
    query_dependents: HashMap<FullName, Vec<FullName>>,
    default_dependents: HashMap<FullName, Vec<FullName>>,
    operator_dependents: HashMap<OperatorIdentity, Vec<FullName>>,
}

#[derive(Clone, Debug)]
struct DependentView(FullName);

impl CascadeRemoval for DependentView {
    fn clone_box(&self) -> Box<dyn CascadeRemoval> {
        Box::new(self.clone())
    }

    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError> {
        context.remove_operator_dependent_view(&self.0)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ViewDependencies {
    query: Vec<FullName>,
    defaults: Vec<FullName>,
    operators: Vec<OperatorIdentity>,
    removal: Option<Box<dyn CascadeRemoval>>,
}

impl ViewDependencies {
    pub(super) fn collect(view: &ViewLike) -> Result<Self, AnalysisError> {
        let mut query = RelationReferences::default();
        view.query().visit(&mut query)?;
        let mut defaults = RelationReferences::default();
        if let Some(view) = view.ordinary_definition() {
            view.visit_defaults(&mut defaults)?;
        }
        Ok(Self {
            query: query.0,
            defaults: defaults.0,
            operators: query.1,
            removal: None,
        })
    }

    fn update_index<K: Clone + Eq + std::hash::Hash>(
        index: &mut HashMap<K, Vec<FullName>>,
        owner: &FullName,
        old: &[K],
        new: &[K],
    ) {
        for name in old {
            if let Some(owners) = index.get_mut(name) {
                owners.retain(|candidate| candidate != owner);
                if owners.is_empty() {
                    index.remove(name);
                }
            }
        }
        for name in new {
            index.entry(name.clone()).or_default().push(owner.clone());
        }
    }
}

#[derive(Default)]
struct RelationReferences(Vec<FullName>, Vec<OperatorIdentity>);

impl SemanticVisitor for RelationReferences {
    fn operator_overload(
        &mut self,
        name: &OperatorName,
        left: Option<&TypeReference>,
        right: &TypeReference,
    ) {
        let identity = OperatorIdentity::bound(name, left, right);
        if !self.1.contains(&identity) {
            self.1.push(identity);
        }
    }

    fn relation_identity(&mut self, name: &FullName) {
        if !self.0.contains(name) {
            self.0.push(name.clone());
        }
    }

    fn schema_type(&mut self, reference: &TypeReference) -> Result<(), AnalysisError> {
        if let Some(name) = reference.full_name() {
            self.relation_identity(name);
        }
        Ok(())
    }
}

impl ViewInformationCollection {
    pub(super) fn append_names(&self, names: &mut Vec<FullName>) {
        for name in &self.names {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    pub(super) fn sqlite_name(&self, name: &FullName) -> Option<FullName> {
        self.names
            .iter()
            .find(|candidate| candidate.sqlite_eq(name))
            .cloned()
    }

    fn get(&self, key: &FullName) -> Option<&ViewLike> {
        self.definitions.get(&key.to_string())
    }

    pub(super) fn register(
        &mut self,
        key: &FullName,
        value: &ViewLike,
        mut dependencies: ViewDependencies,
    ) {
        let previous = self.dependencies.remove(key).unwrap_or_default();
        ViewDependencies::update_index(
            &mut self.query_dependents,
            key,
            &previous.query,
            &dependencies.query,
        );
        ViewDependencies::update_index(
            &mut self.default_dependents,
            key,
            &previous.defaults,
            &dependencies.defaults,
        );
        ViewDependencies::update_index(
            &mut self.operator_dependents,
            key,
            &previous.operators,
            &dependencies.operators,
        );
        dependencies.removal = Some(Box::new(DependentView(key.clone())));
        self.dependencies.insert(key.clone(), dependencies);
        self.insert(key, value);
    }

    pub(super) fn insert(&mut self, key: &FullName, value: &ViewLike) {
        self.definitions.insert(&key.to_string(), value.clone());
        if !self.names.contains(key) {
            self.names.push(key.clone());
        }
    }
}

impl SchemaFileContext {
    fn remove_operator_dependent_view(&mut self, name: &FullName) -> Result<(), AnalysisError> {
        let Some(definition) = self.get_view_impl(name).cloned() else {
            return AnalysisError::raise_unexpected_input(
                "resolved dependent view missing from catalog",
            );
        };
        self.catalog
            .mask_view(name.clone(), ViewTombstone::for_operator_drop(definition));
        Ok(())
    }
}

impl SchemaCatalog {
    pub(super) fn mask_view(&mut self, name: FullName, tombstone: ViewTombstone) {
        // The relation and its composite row type disappear together.
        self.renamed_types.insert(name.clone());
        self.view_tombstones.insert(name, tombstone);
    }
}

impl ViewInfoRead for SchemaCatalog {
    fn operator_view_dependents(&self, operator: &OperatorIdentity) -> Vec<FullName> {
        let mut owners = self.base.operator_view_dependents(operator);
        owners.retain(|owner| !self.views.dependencies.contains_key(owner));
        if let Some(local) = self.views.operator_dependents.get(operator) {
            owners.extend(local.iter().cloned());
        }
        owners.retain(|owner| self.get_view_impl(owner).is_some());
        owners
    }

    fn view_cascade_removal(&self, name: &FullName) -> Option<Box<dyn CascadeRemoval>> {
        self.get_view_impl(name)?;
        if let Some(dependencies) = self.views.dependencies.get(name) {
            dependencies.removal.clone()
        } else {
            self.base.view_cascade_removal(name)
        }
    }

    fn view_tombstone(&self, key: &FullName) -> Option<&ViewTombstone> {
        let key = self.relation_identity(key);
        if let Some(tombstone) = self.view_tombstones.get(&key) {
            return Some(tombstone);
        }
        if self.objects.get(&key.to_string()).is_some() {
            return None;
        }
        self.base.view_tombstone(&key)
    }

    fn view_dependents(&self, name: &FullName) -> ViewDependents {
        let mut dependents = self.base.view_dependents(name);
        for (owners, index) in [
            (&mut dependents.queries, &self.views.query_dependents),
            (&mut dependents.defaults, &self.views.default_dependents),
        ] {
            owners.retain(|owner| !self.views.dependencies.contains_key(owner));
            if let Some(local) = index.get(name) {
                owners.extend(local.iter().cloned());
            }
            owners.retain(|owner| {
                matches!(
                    self.object_kind(owner),
                    Some(Kind::View | Kind::MaterializedView)
                )
            });
        }
        dependents
    }

    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        let identity = self.relation_identity(key);
        let key = &identity;
        if self.view_tombstones.contains_key(key) {
            return None;
        }
        if let Some(kind) = self.objects.get(&key.to_string())
            && *kind != Kind::view()
            && *kind != Kind::materialized_view()
        {
            return None;
        }
        if let Some(v) = self.views.get(key) {
            Some(v)
        } else {
            self.base.get_view_impl(key)
        }
    }
}

impl ViewInfoWrite for SchemaFileContext {
    fn apply_drop_view(mut self, statement: &DropView) -> Result<Self, AnalysisError> {
        if !statement.applicable() {
            return Ok(self);
        }
        let cause = Arc::new(statement.clone());
        let mut staged = Vec::new();
        for name in statement.removals() {
            let Some(view) = self.get_view_impl(name).cloned() else {
                return AnalysisError::raise_unexpected_input("DROP VIEW definition missing");
            };
            staged.push((name.clone(), ViewTombstone::for_view_drop(view, &cause)));
        }
        for (name, tombstone) in staged {
            if let Some(rules) = tombstone.definition().rules() {
                for rule in rules.identities() {
                    self = self.cancel_rule(rule)?;
                }
            }
            if let Some(triggers) = tombstone.definition().triggers() {
                for trigger in triggers.names() {
                    self = self.cancel_trigger(&TriggerIdentity::postgres(&name, trigger))?;
                }
            }
            let kind = if tombstone.definition().ordinary_definition().is_some() {
                ObjectKind::VIEW
            } else {
                ObjectKind::MATERIALIZED_VIEW
            };
            let member = OwnershipIdentity::named(kind, &name);
            if let Some(extension) = self.extension_membership(&member) {
                let change = ExtensionMembershipChange::removed(
                    extension,
                    &member,
                    statement.source().as_ref(),
                );
                self.catalog.apply_extension_membership(&change);
            }
            self.catalog.mask_view(name, tombstone);
        }
        Ok(self)
    }

    #[tracing::instrument(skip(self))]
    fn apply_alter_view(mut self, alter_view: &AlterView) -> Result<Self, AnalysisError>
    where
        Self: Sized,
    {
        let view_name = alter_view.view();
        let Some((key, mut view)) =
            expand_relation_search_path(&self, view_name.full_name(), |key| {
                self.get_view_impl(key)
                    .cloned()
                    .map(|view| (key.clone(), view))
            })
        else {
            self.report_analysis_problem(AnalysisProblem::view_not_found(view_name))?;
            return Ok(self);
        };
        if !view.apply_alter(&mut self, alter_view)? {
            return Ok(self);
        }
        self.catalog.store_view(&key, &view)?;
        Ok(self)
    }

    fn apply_create_table_as(
        mut self,
        create_table_as: &CreateTableAs,
    ) -> Result<Self, AnalysisError> {
        if !create_table_as.set_operations_valid()? {
            return Ok(self);
        }
        let name = self.apply_default_schema(create_table_as.name())?;
        match create_table_as.obj_type() {
            Kind::MaterializedView => {
                if !self.permits_relation_creation(
                    &name,
                    Kind::materialized_view(),
                    create_table_as.if_not_exists(),
                    false,
                )? {
                    return Ok(self);
                }
                self.catalog
                    .store_view(&name, &ViewLike::from(create_table_as))?;
                self.record_named_extension_member(ObjectKind::MATERIALIZED_VIEW, &name);
                Ok(self)
            }
            _ => {
                todo!()
            }
        }
    }

    #[tracing::instrument(skip(self))]
    fn apply_create_view(mut self, create_view: &CreateView) -> Result<Self, AnalysisError> {
        if !create_view.query().set_operations_valid()? {
            return Ok(self);
        }
        let name = self.creation_name(create_view.name(), create_view.temporary())?;
        let kind = if create_view.is_materialized() {
            Kind::materialized_view()
        } else {
            Kind::view()
        };
        if !self.permits_relation_creation(
            &name,
            kind,
            create_view.if_not_exists(),
            create_view.replace(),
        )? {
            return Ok(self);
        }
        let is_new = self.get_view_impl(&name).is_none();
        let mut definition = ViewLike::from(create_view);
        if create_view.replace()
            && let Some(previous) = self.get_view_impl(&name)
        {
            if !definition
                .result_columns()
                .can_replace_view_columns(previous.result_columns())
            {
                self.report_analysis_problem(AnalysisProblem::creation(CreationProblem::named(
                    &name,
                    "view replacement",
                    &self.creation_span(),
                )))?;
                return Ok(self);
            }
            if let Some(previous) = previous.ordinary_definition()
                && let ViewLike::View(current) = &mut definition
            {
                current.retain_relation_state(previous);
            }
            if let Some(owner) = previous.owner() {
                definition.change_owner(owner);
            }
        }
        self.catalog.store_view(&name, &definition)?;
        if is_new {
            let kind = if create_view.is_materialized() {
                ObjectKind::MATERIALIZED_VIEW
            } else {
                ObjectKind::VIEW
            };
            self.record_named_extension_member(kind, &name);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod drop_view_tests {
    use std::path::Path;

    use crate::{
        ast::ObjectKind,
        context::{
            ExtensionInfoRead, MutSourcePathWithSpan, ParseContext, TypeInfoRead, ViewInfoRead,
        },
        names::{FullName, SchemaName},
        sem::{DropViewLookup, OwnershipIdentity, SemAst, SourcePathWithSpan, ViewDropCause},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer, source_span,
        },
    };

    #[rstest::rstest]
    #[case::restriction("DROP VIEW public.base", Some("drop_view_dependency"))]
    #[case::missing("DROP VIEW public.base, missing CASCADE", Some("view_not_found"))]
    #[case::wrong_kind(
        "DROP VIEW IF EXISTS public.base, public.other CASCADE",
        Some("drop_view_wrong_kind")
    )]
    #[case::cascade("DROP VIEW public.base CASCADE", None)]
    #[case::group("DROP VIEW public.leaf, public.base, public.base", None)]
    fn applies_the_group_atomically(
        #[case] sql: &str,
        #[case] diagnostic: Option<&str>,
    ) -> testresult::TestResult {
        let applicable = diagnostic.is_none();
        let mut context = ownership_context();
        for ddl in [
            "CREATE VIEW public.base AS SELECT 1 AS n",
            "CREATE VIEW public.leaf AS SELECT * FROM public.base",
            "CREATE TABLE public.other (n integer)",
        ] {
            context = apply_ownership_sql(context, ddl)?.into_context();
            assert!(context.problems().is_empty());
        }
        let parent = context.freeze();
        let path = Path::new("drop-views.sql");
        let mut layer = schema_layer(path, &parent);
        layer.set_source_path_with_span(&SourcePathWithSpan::new(path, source_span().span()));
        let result = apply_ownership_sql(layer, sql)?;
        let SemAst::DropView(statement) = result.ast() else {
            panic!("expected DROP VIEW");
        };
        assert_eq!(statement.applicable(), applicable);
        assert_eq!(statement.source().as_ref().unwrap().source_path(), path);
        assert!(matches!(
            statement.targets()[0].lookup(),
            DropViewLookup::Resolved(_)
        ));
        let codes: Vec<_> = result
            .context()
            .problems()
            .iter()
            .map(|problem| problem.code())
            .collect();
        assert_eq!(codes, diagnostic.into_iter().collect::<Vec<_>>());

        let child = result.into_context().freeze();
        for name in ["base", "leaf"] {
            let name = FullName::with_schema("public", name);
            assert!(parent.get_view_impl(&name).is_some());
            assert!(parent.get_type_impl(&name).is_some());
            assert_eq!(child.get_view_impl(&name).is_none(), applicable);
            assert_eq!(child.get_type_impl(&name).is_none(), applicable);
            if applicable {
                let tombstone = child.view_tombstone(&name).unwrap();
                let ViewDropCause::View(cause) = tombstone.cause() else {
                    panic!("expected DROP VIEW provenance");
                };
                assert_eq!(cause.source().as_ref().unwrap().source_path(), path);
            } else {
                assert!(child.view_tombstone(&name).is_none());
            }
        }
        Ok(())
    }

    #[test]
    fn only_the_owning_extension_script_can_drop_its_member() -> testresult::TestResult {
        let mut install = ownership_context();
        install.enter_extension_script("extension_one", Some("public"));
        let parent = apply_ownership_sql(install, "CREATE VIEW public.member AS SELECT 1 AS n")?
            .into_context()
            .freeze();
        let name = FullName::with_schema("public", "member");
        let identity = OwnershipIdentity::named(ObjectKind::VIEW, &name);
        let path = Path::new("extension-update.sql");
        let outside = apply_ownership_sql(
            schema_layer(path, &parent),
            "DROP VIEW public.member CASCADE",
        )?;
        assert!(outside.context().get_view_impl(&name).is_some());
        assert!(!outside.context().problems().is_empty());

        let mut update = schema_layer(path, &parent);
        update.enter_extension_script("extension_one", Some("public"));
        let dropped = apply_ownership_sql(update, "DROP VIEW public.member")?.into_context();
        assert!(dropped.problems().is_empty());
        assert!(dropped.get_view_impl(&name).is_none());
        assert_eq!(dropped.extension_membership(&identity), None);
        assert_eq!(
            parent.extension_membership(&identity),
            Some("extension_one")
        );

        let recreated = apply_ownership_sql(dropped, "CREATE VIEW public.member AS SELECT 2 AS n")?
            .into_context();
        assert!(recreated.problems().is_empty());
        assert!(recreated.view_tombstone(&name).is_none());
        assert_eq!(
            recreated.extension_membership(&identity),
            Some("extension_one")
        );
        Ok(())
    }

    #[test]
    fn search_path_shadows_views_and_if_exists_accepts_missing_targets() -> testresult::TestResult {
        let mut context = ownership_context();
        for ddl in [
            "CREATE SCHEMA shadow",
            "CREATE TABLE shadow.v (n integer)",
            "CREATE VIEW public.v AS SELECT 1 AS n",
        ] {
            context = apply_ownership_sql(context, ddl)?.into_context();
            assert!(context.problems().is_empty());
        }
        context.set_search_path(&[SchemaName::from("shadow"), SchemaName::from("public")]);
        let parent = context.freeze();
        let path = Path::new("view-path.sql");
        let rejected = apply_ownership_sql(schema_layer(path, &parent), "DROP VIEW IF EXISTS v")?;
        let SemAst::DropView(statement) = rejected.ast() else {
            panic!("expected DROP VIEW");
        };
        assert!(!statement.applicable());
        assert!(matches!(
            statement.targets()[0].lookup(),
            DropViewLookup::WrongKind(name, _) if name == &FullName::with_schema("shadow", "v")
        ));

        let dropped =
            apply_ownership_sql(schema_layer(path, &parent), "DROP VIEW public.v")?.into_context();
        assert!(dropped.problems().is_empty());
        let skipped = apply_ownership_sql(dropped, "DROP VIEW IF EXISTS public.v, missing")?;
        let SemAst::DropView(statement) = skipped.ast() else {
            panic!("expected DROP VIEW");
        };
        assert!(statement.applicable());
        assert!(statement.removals().is_empty());
        assert!(
            statement
                .targets()
                .iter()
                .all(|target| matches!(target.lookup(), DropViewLookup::Missing))
        );
        assert!(skipped.context().problems().is_empty());
        Ok(())
    }
}

#[cfg(test)]
mod test_replacement_preserves_view_privileges {
    use crate::{
        context::ViewInfoRead,
        names::FullName,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn replacement_preserves_privilege_history() -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE VIEW public.v AS SELECT 1 AS id",
        )?
        .into_context();
        let context =
            apply_ownership_sql(context, "GRANT SELECT ON public.v TO PUBLIC")?.into_context();
        let result =
            apply_ownership_sql(context, "CREATE OR REPLACE VIEW public.v AS SELECT 2 AS id")?;
        assert!(result.context().problems().is_empty());
        let name = FullName::with_schema("public", "v");
        assert_eq!(
            result
                .context()
                .get_view_impl(&name)
                .unwrap()
                .ordinary_definition()
                .unwrap()
                .privilege_declarations()
                .len(),
            1
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_set_operation_catalog {
    use crate::{
        ast::Dialect,
        context::ViewInfoRead,
        names::FullName,
        sem::PgBuiltInType,
        test_helpers::{ownership::apply_ownership_sql, set_operations},
    };

    #[test]
    fn rejected_set_queries_preserve_catalog_and_allow_following_statements()
    -> testresult::TestResult {
        let context = set_operations::context(Dialect::postgres());
        let initial = apply_ownership_sql(context, "CREATE VIEW kept AS SELECT 1::bigint AS x")?;
        let rejected = apply_ownership_sql(
            initial.into_context(),
            "CREATE OR REPLACE VIEW kept AS SELECT 1 AS x UNION SELECT 'x'::text",
        )?;
        let name = FullName::with_schema("public", "kept");
        let kept = rejected.context().get_view_impl(&name).unwrap();
        assert_eq!(
            kept.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int8())
        );
        let rejected = apply_ownership_sql(
            rejected.into_context(),
            "CREATE MATERIALIZED VIEW absent AS SELECT 1 UNION SELECT 1,2",
        )?;
        assert!(
            rejected
                .context()
                .get_view_impl(&FullName::with_schema("public", "absent"))
                .is_none()
        );
        let recovered = apply_ownership_sql(
            rejected.into_context(),
            "CREATE VIEW next AS SELECT 1 AS x UNION SELECT 2::bigint",
        )?;
        let next = recovered
            .context()
            .get_view_impl(&FullName::with_schema("public", "next"))
            .unwrap();
        assert_eq!(
            next.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int8())
        );
        assert_eq!(recovered.context().problems().len(), 2);
        Ok(())
    }
}
