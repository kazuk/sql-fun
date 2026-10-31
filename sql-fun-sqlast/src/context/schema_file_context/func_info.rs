use crate::context::schema_file_context::{SchemaCatalog, SchemaFileContext};
use crate::{
    TrieMap,
    context::{FunctionInfoRead, FunctionInfoWrite, ParseContext},
    sem::{AnalysisError, CreateFunction, FullName, FunctionOverloadCollection},
};

#[derive(Clone, Debug, Default)]
pub struct FunctionDefinitionCollection(TrieMap<Vec<CreateFunction>>, Vec<FullName>);

impl FunctionDefinitionCollection {
    fn append_names(&self, names: &mut Vec<FullName>) {
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
    fn declared_routine_names(&self) -> Vec<FullName> {
        let mut names = self.base.declared_routine_names();
        self.functions.append_names(&mut names);
        names
    }

    fn is_aggregate_impl(&self, name: &FullName) -> bool {
        if self.functions.definitions(name).is_empty() {
            self.base.is_aggregate_impl(name)
        } else {
            false
        }
    }
    fn function_returns_set(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool {
        let arguments = overload.input_bindings();
        for definition in self.functions.definitions(name) {
            let selected = definition
                .overloads()
                .resolve_from_catalog(self, &arguments);
            if selected.as_ref() == Some(overload) {
                return definition.is_table_function();
            }
        }
        self.base.function_returns_set(name, overload)
    }

    fn function_preserves_view_rows(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool {
        if let Some(preserves) = self
            .function_view_rows
            .get(&(name.clone(), overload.clone()))
        {
            return *preserves;
        }
        // Recover the producer signature through the existing overload binder:
        // parameter inference specializes polymorphic signatures, while source
        // properties continue to belong to their original overload identity.
        let arguments = overload.input_bindings();
        for ((function, signature), preserves) in &self.function_view_rows {
            if function == name {
                let candidate = FunctionOverloadCollection::new(std::slice::from_ref(signature))
                    .resolve_from_catalog(self, &arguments);
                if candidate.as_ref() == Some(overload) {
                    return *preserves;
                }
            }
        }
        self.base.function_preserves_view_rows(name, overload)
    }

    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        let definitions = crate::context::OwnershipInfoRead::routine_definitions(self, name);
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

#[cfg(test)]
mod test_expression_classification {
    use crate::context::{BaseParseContext, FunctionInfoRead, ParseContext};
    use crate::sem::FullName;
    use crate::test_helpers::{ownership::apply_ownership_sql, sampling::schema_context};

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
        let existing = crate::context::OwnershipInfoRead::routine_definitions(&self, name);
        let mut candidates = Vec::new();
        for mut candidate in create_function.split_definitions() {
            if let Some(previous) = candidate.previous_definition(&existing, &candidates) {
                if !create_function.replace() || !candidate.can_replace(previous) {
                    self.report_analysis_problem(crate::sem::AnalysisProblem::creation(
                        crate::sem::problem::CreationProblem::routine(
                            name,
                            &candidate.ownership_identity(name),
                            &self.creation_span(),
                        ),
                    ))?;
                    return Ok(self);
                }
                if let Some(owner) = previous.owner() {
                    candidate.change_owner(owner);
                }
            }
            candidates.push(candidate);
        }
        for candidate in candidates {
            self.catalog.functions.insert(name, &candidate);
        }
        Ok(self)
    }
}
