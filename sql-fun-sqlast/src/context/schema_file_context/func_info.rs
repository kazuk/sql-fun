use crate::{
    TrieMap,
    ast::ObjectKind,
    context::{
        FunctionInfoRead, FunctionInfoWrite, OwnershipInfoRead, ParseContext, ViewInfoRead,
        schema_file_context::{SchemaCatalog, SchemaFileContext},
    },
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CreateFunction, DropFunction, FunctionOverloadCollection,
        FunctionTombstone, OverloadVariant, OwnershipIdentity, ViewTombstone,
        problem::CreationProblem,
    },
};

#[derive(Clone, Debug, Default)]
pub struct FunctionDefinitionCollection(TrieMap<Vec<CreateFunction>>, Vec<FullName>);

impl FunctionDefinitionCollection {
    pub(super) fn append_names(&self, names: &mut Vec<FullName>) {
        for name in &self.1 {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    pub(super) fn replace(&mut self, name: &FullName, definitions: &[CreateFunction]) {
        self.0.insert(&name.to_string(), definitions.to_vec());
        if !self.1.contains(name) {
            self.1.push(name.clone());
        }
    }

    pub(super) fn definitions(&self, name: &FullName) -> Vec<CreateFunction> {
        self.0.get(&name.to_string()).cloned().unwrap_or_default()
    }
    pub(super) fn insert(&mut self, name: &FullName, create_function: &CreateFunction) {
        let mut definitions = self.definitions(name);
        for definition in create_function.split_definitions() {
            let identity = definition.ownership_identity(name);
            definitions.retain(|existing| existing.ownership_identity(name) != identity);
            definitions.push(definition);
        }
        self.replace(name, &definitions);
    }
}

impl FunctionInfoRead for SchemaCatalog {
    fn function_tombstone(&self, identity: &OwnershipIdentity) -> Option<&FunctionTombstone> {
        if let Some(tombstone) = self.function_tombstones.get(identity) {
            return Some(tombstone);
        }
        if CreateFunction::contains_identity(
            &self.functions.definitions(identity.name()),
            identity.name(),
            identity,
        ) {
            return None;
        }
        self.base.function_tombstone(identity)
    }

    fn declared_routine_names(&self) -> Vec<FullName> {
        let mut names = self.base.declared_routine_names();
        self.functions.append_names(&mut names);
        names
    }

    fn is_aggregate_impl(&self, name: &FullName) -> bool {
        if self
            .function_tombstones
            .keys()
            .any(|identity| identity.name() == name)
        {
            return self
                .routine_definitions(name)
                .iter()
                .any(CreateFunction::catalog_aggregate);
        }
        let local = self.functions.definitions(name);
        if local.is_empty() {
            self.base.is_aggregate_impl(name)
        } else {
            local.iter().any(CreateFunction::catalog_aggregate)
        }
    }
    fn function_returns_set(&self, name: &FullName, overload: &OverloadVariant) -> bool {
        if self.masks_overload(name, overload) {
            return false;
        }
        for definition in self.functions.definitions(name) {
            if definition
                .overloads()
                .contains_instantiation(overload, self)
            {
                return definition.catalog_returns_set();
            }
        }
        self.base.function_returns_set(name, overload)
    }

    fn function_preserves_view_rows(&self, name: &FullName, overload: &OverloadVariant) -> bool {
        if self.masks_overload(name, overload) {
            return false;
        }
        if let Some(preserves) = self
            .function_view_rows
            .get(&(name.clone(), overload.clone()))
        {
            return *preserves;
        }
        // A resolved overload may be a polymorphic instantiation of its
        // declaration. Its row-preservation property belongs to that declaration.
        for ((function, signature), preserves) in &self.function_view_rows {
            if function == name
                && FunctionOverloadCollection::new(std::slice::from_ref(signature))
                    .contains_instantiation(overload, self)
            {
                return *preserves;
            }
        }
        self.base.function_preserves_view_rows(name, overload)
    }

    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        let definitions = OwnershipInfoRead::routine_definitions(self, name);
        if definitions.is_empty() {
            return None;
        }
        let mut overloads = FunctionOverloadCollection::default();
        for definition in definitions {
            if definition.object_kind().code() == 20 {
                overloads.append(definition.overloads());
            }
        }
        if overloads.is_empty() {
            None
        } else {
            Some(overloads)
        }
    }
}

impl SchemaCatalog {
    fn masks_overload(&self, name: &FullName, overload: &OverloadVariant) -> bool {
        if self.function_tombstones.is_empty() {
            return false;
        }
        let arguments = overload.declaration_arguments();
        let identity = OwnershipIdentity::with_arguments(ObjectKind::FUNCTION, name, &arguments);
        self.function_tombstones.contains_key(&identity)
    }
}

#[cfg(test)]
mod test_expression_classification {
    use crate::{
        context::{BaseParseContext, FunctionInfoRead, ParseContext},
        names::FullName,
        test_helpers::{ownership::apply_ownership_sql, sampling::schema_context},
    };

    #[rstest::rstest]
    #[case::simple("anyelement", "anyelement", "1", false)]
    #[case::compatible_set("anycompatible", "SETOF anycompatible", "1", true)]
    #[case::compatible_variadic("VARIADIC anycompatiblearray", "anycompatible", "1,2", false)]
    fn instantiated_signature_keeps_declared_properties_after_freeze(
        #[case] parameters: &str,
        #[case] result: &str,
        #[case] argument: &str,
        #[case] returns_set: bool,
    ) -> testresult::TestResult {
        use crate::test_helpers::function_overloads as test_helpers;

        let analyzed =
            test_helpers::analyze_vector(&format!("SELECT vf({argument})"), parameters, result)?;
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let overload = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_function_call()
            .unwrap()
            .overload()
            .clone()
            .unwrap();
        let name = FullName::with_schema("public", "vf");
        assert_eq!(
            analyzed.context().function_returns_set(&name, &overload),
            returns_set
        );
        assert_eq!(
            analyzed
                .context()
                .function_preserves_view_rows(&name, &overload),
            !returns_set
        );
        let frozen = analyzed.into_context().freeze();
        assert_eq!(frozen.function_returns_set(&name, &overload), returns_set);
        assert_eq!(
            frozen.function_preserves_view_rows(&name, &overload),
            !returns_set
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::scalar("integer", "", false, true)]
    #[case::set("SETOF integer", "", true, false)]
    #[case::window("integer", "WINDOW", false, false)]
    fn declared_return_shape_is_independent_of_view_rows(
        #[case] result: &str,
        #[case] options: &str,
        #[case] returns_set: bool,
        #[case] preserves_rows: bool,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            schema_context(),
            &format!("CREATE FUNCTION public.property507(integer) RETURNS {result} {options} LANGUAGE internal AS 'abs'"),
        )?.into_context();
        let name = FullName::with_schema("public", "property507");
        let overloads = context.get_function_by_name(&name).unwrap();
        let overload = overloads.get_at(0).unwrap();
        assert_eq!(context.function_returns_set(&name, overload), returns_set);
        assert_eq!(
            context.function_preserves_view_rows(&name, overload),
            preserves_rows
        );
        let frozen = context.freeze();
        assert_eq!(frozen.function_returns_set(&name, overload), returns_set);
        Ok(())
    }
}

impl FunctionInfoWrite for SchemaFileContext {
    fn apply_drop_function(mut self, statement: &DropFunction) -> Result<Self, AnalysisError> {
        if !statement.applicable() {
            return Ok(self);
        }
        let cause = std::sync::Arc::new(statement.clone());
        let mut removals = Vec::new();
        for identity in statement.targets() {
            let Some(definition) = self
                .routine_definitions(identity.name())
                .into_iter()
                .find(|definition| definition.ownership_identity(identity.name()) == *identity)
            else {
                return AnalysisError::raise_unexpected_input(
                    "DROP FUNCTION definition missing from catalog",
                );
            };
            removals.push((identity.clone(), FunctionTombstone::new(definition, &cause)));
        }
        let mut views = Vec::new();
        for name in statement.dependent_views() {
            let Some(definition) = self.get_view_impl(name).cloned() else {
                return AnalysisError::raise_unexpected_input(
                    "DROP FUNCTION dependent view missing from catalog",
                );
            };
            views.push((name.clone(), ViewTombstone::new(definition, &cause)));
        }
        for (name, tombstone) in views {
            // The relation's composite row type disappears with the view.
            self.catalog.renamed_types.insert(name.clone());
            self.catalog.view_tombstones.insert(name, tombstone);
        }
        self.catalog.function_tombstones.extend(removals);
        Ok(self)
    }

    fn apply_function_view_rows(
        mut self,
        function: &CreateFunction,
        preserves_rows: bool,
    ) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(function.name())?;
        self.catalog.function_view_rows.extend(
            function
                .overloads()
                .view_row_properties(&name, preserves_rows),
        );
        Ok(self)
    }

    #[tracing::instrument(skip(self))]
    fn apply_create_function_impl(
        mut self,
        name: &FullName,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError> {
        let existing = OwnershipInfoRead::routine_definitions(&self, name);
        let mut candidates = Vec::new();
        let mut new_members = Vec::new();
        for mut candidate in create_function.split_definitions() {
            let previous = candidate.previous_definition(&existing, &candidates);
            let is_new = previous.is_none();
            if let Some(previous) = previous {
                if !create_function.replace() || !candidate.can_replace(previous) {
                    self.report_analysis_problem(AnalysisProblem::creation(
                        CreationProblem::routine(
                            name,
                            &candidate.ownership_identity(name),
                            &self.creation_span(),
                        ),
                    ))?;
                    return Ok(self);
                }
                candidate.retain_privilege_declarations(previous);
                if let Some(owner) = previous.owner() {
                    candidate.change_owner(owner);
                }
            }
            if is_new {
                new_members.push(candidate.ownership_identity(name));
            }
            candidates.push(candidate);
        }
        for candidate in candidates {
            self.catalog
                .function_tombstones
                .remove(&candidate.ownership_identity(name));
            self.catalog.functions.insert(name, &candidate);
        }
        for member in new_members {
            self.record_extension_member(&member);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod test_drop_function {
    use std::path::Path;

    use crate::{
        ast::ObjectKind,
        context::{FunctionInfoRead, MutSourcePathWithSpan, ParseContext, ViewInfoRead},
        names::FullName,
        sem::{OwnershipIdentity, PgBuiltInType, SemAst, SourcePathWithSpan},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer, source_span,
        },
    };

    #[test]
    fn rejected_drop_is_atomic_and_cascade_retains_shared_provenance() -> testresult::TestResult {
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
        let context =
            apply_ownership_sql(context, "CREATE VIEW public.v AS SELECT public.f(1) AS n")?
                .into_context();
        let parent = context.freeze();
        let function = FullName::with_schema("public", "f");
        let view = FullName::with_schema("public", "v");
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::FUNCTION,
            &function,
            &[Some(PgBuiltInType::int4())],
        );

        let restricted = apply_ownership_sql(
            schema_layer(Path::new("restrict.sql"), &parent),
            "DROP FUNCTION public.f(integer) RESTRICT",
        )?;
        assert!(!restricted.context().problems().is_empty());
        let SemAst::DropFunction(statement) = restricted.ast() else {
            panic!("expected analyzed DROP FUNCTION");
        };
        assert!(!statement.applicable());
        assert_eq!(statement.dependent_views(), std::slice::from_ref(&view));
        assert!(restricted.context().function_tombstone(&identity).is_none());
        assert!(restricted.context().get_view_impl(&view).is_some());

        let cascade_path = Path::new("cascade.sql");
        let mut context = schema_layer(cascade_path, &parent);
        let drop_source = SourcePathWithSpan::new(cascade_path, source_span().span());
        context.set_source_path_with_span(&drop_source);
        let result = apply_ownership_sql(context, "DROP FUNCTION public.f(integer) CASCADE")?;
        assert!(result.context().problems().is_empty());
        let child = result.into_context().freeze();
        let function_tombstone = child.function_tombstone(&identity).unwrap();
        let view_tombstone = child.view_tombstone(&view).unwrap();
        assert!(function_tombstone.definition().source().is_some());
        assert_eq!(
            function_tombstone.cause().source().unwrap().source_path(),
            cascade_path
        );
        assert!(std::ptr::eq(
            function_tombstone.cause(),
            view_tombstone.cause()
        ));
        assert!(child.get_view_impl(&view).is_none());
        assert!(parent.get_view_impl(&view).is_some());
        assert!(parent.function_tombstone(&identity).is_none());
        assert!(
            child
                .get_function_impl(&function)
                .unwrap()
                .get_at(1)
                .is_none()
        );

        let recreated = apply_ownership_sql(
            schema_layer(Path::new("recreate.sql"), &child),
            "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?;
        assert!(recreated.context().problems().is_empty());
        assert!(recreated.context().function_tombstone(&identity).is_none());
        assert!(
            recreated
                .context()
                .get_function_impl(&function)
                .unwrap()
                .get_at(1)
                .is_some()
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_replacement_preserves_routine_privileges {
    use crate::{
        context::OwnershipInfoRead,
        names::FullName,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn replacement_preserves_privilege_history() -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.v(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "GRANT EXECUTE ON FUNCTION public.v(integer) TO PUBLIC",
        )?
        .into_context();
        let result = apply_ownership_sql(
            context,
            "CREATE OR REPLACE FUNCTION public.v(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1 + 1'",
        )?;
        assert!(result.context().problems().is_empty());
        let name = FullName::with_schema("public", "v");
        assert_eq!(
            result.context().routine_definitions(&name)[0]
                .privilege_declarations()
                .len(),
            1
        );
        Ok(())
    }
}
