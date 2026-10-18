use crate::sem::{
    AnalysisError, CreateFunction, FullName, FunctionInfoRead, FunctionInfoWrite,
    FunctionOverloadCollection,
};

use crate::TrieMap;

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
pub struct FunctionDefinitionCollection(TrieMap<Vec<CreateFunction>>);

impl FunctionDefinitionCollection {
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
        self.0.insert(&name.to_string(), definitions);
    }
}

impl FunctionInfoRead for SchemaCatalog {
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        let definitions = crate::sem::OwnershipInfoRead::routine_definitions(self, name);
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

impl FunctionInfoWrite for SchemaFileContext {
    #[tracing::instrument(skip(self))]
    fn apply_create_function_impl(
        mut self,
        name: &FullName,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError> {
        self.catalog.functions.insert(name, create_function);
        Ok(self)
    }
}
