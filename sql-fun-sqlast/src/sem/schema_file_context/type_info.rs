use crate::{
    TrieMap,
    sem::{
        BaseParseContext, CreateCompositeType, CreateDomain, CreateEnum, FullName, TableName,
        TypeDefinition, TypeInfoRead, TypeInfoWrite, TypeReference,
    },
};

use super::{SchemaCatalog, SchemaFileContext};

impl TypeInfoRead for SchemaCatalog {
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.types.get_type_impl(key, &self.base)
    }

    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference> {
        Some(TypeReference::from_full_name(type_name.full_name().clone()))
    }
}

impl TypeInfoRead for SchemaFileContext {
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.catalog.get_type_impl(key)
    }

    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference> {
        self.catalog.get_array_item_type(type_name)
    }
}

impl TypeInfoWrite for SchemaFileContext {
    fn insert_table_ref_type(&mut self, name: &TableName) {
        self.catalog.types.insert_table_ref(name);
    }

    fn insert_create_composite_type(
        &mut self,
        name: &FullName,
        create_comp_type: &CreateCompositeType,
    ) {
        self.catalog
            .types
            .insert_create_composite_type(name, create_comp_type);
    }

    fn insert_create_domain(&mut self, name: &FullName, create_domain: &CreateDomain) {
        self.catalog.types.insert_create_domain(name, create_domain);
    }

    fn insert_create_enum(&mut self, name: &FullName, create_enum: &CreateEnum) {
        self.catalog.types.insert_create_enum(name, create_enum);
    }
}

#[derive(Debug, Default)]
pub(super) struct TypeDefinitionCollection(TrieMap<TypeDefinition>);

impl TypeDefinitionCollection {
    pub(super) fn get_type_impl<'a, TBaseContext>(
        &'a self,
        key: &FullName,
        base: &'a TBaseContext,
    ) -> Option<&'a TypeDefinition>
    where
        TBaseContext: BaseParseContext + std::fmt::Debug,
    {
        if let Some(v) = self.0.get(&key.to_string()) {
            Some(v)
        } else {
            base.get_type_impl(key)
        }
    }

    pub(super) fn insert_table_ref(&mut self, table_name: &TableName) {
        let table_ref = TypeDefinition::table_ref(table_name);
        self.0.insert(&table_name.to_string(), table_ref);
    }

    pub(super) fn insert_create_enum(&mut self, name: &FullName, create_enum: &CreateEnum) {
        self.0
            .insert(&name.to_string(), TypeDefinition::from(create_enum));
    }

    pub(super) fn insert_create_composite_type(
        &mut self,
        name: &FullName,
        create_comp_type: &CreateCompositeType,
    ) {
        self.0
            .insert(&name.to_string(), TypeDefinition::from(create_comp_type));
    }

    pub(super) fn insert_create_domain(&mut self, name: &FullName, create_domain: &CreateDomain) {
        self.0
            .insert(&name.to_string(), TypeDefinition::from(create_domain));
    }
}
