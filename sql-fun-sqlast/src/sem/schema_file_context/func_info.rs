use crate::{
    TrieMap,
    sem::{
        AnalysisError, CreateFunction, FullName, FunctionInfoRead, FunctionInfoWrite,
        FunctionOverloadCollection,
    },
};

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
pub struct FunctionDefinitionCollection(TrieMap<FunctionOverloadCollection>);

impl FunctionDefinitionCollection {
    pub fn get(&self, name: &FullName) -> Option<&FunctionOverloadCollection> {
        self.0.get(&name.to_string())
    }

    pub fn insert(&mut self, name: &FullName, create_function: &CreateFunction) {
        let key = &name.to_string();
        if let Some(overload_collection) = self.0.get_mut(key) {
            overload_collection.append(create_function.overloads());
        } else {
            let mut overload_collection = FunctionOverloadCollection::default();
            overload_collection.append(create_function.overloads());
            self.0.insert(key, overload_collection);
        }
    }
}

impl FunctionInfoRead for SchemaCatalog {
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        let self_funcs = self.functions.get(name);
        let base_funcs = self.base.get_function_impl(name);

        match (self_funcs, base_funcs) {
            (Some(s), Some(b)) => Some(s.clone().merged(b)),
            (Some(s), None) => Some(s.clone()),
            (None, Some(b)) => Some(b.clone()),
            (None, None) => None,
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
