use std::path::Path;

use ::sql_fun_core::SqlFunArgs;

use crate::{
    context::{
        CastInfoRead, ConflictInfoRead, FunctionInfoRead, IndexInfoRead, OperatorInfoRead,
        RuleInfoRead, SearchPathRead, SequenceInfoRead, TableInfoRead, TriggerInfoRead,
        TypeInfoRead, ViewInfoRead,
    },
    sem::{
        CreateIndex, CreateSequence, CreateTable, FullName, FunctionOverloadCollection,
        GetSourcePathWithSpanOpt, TableName, TypeDefinition, TypeReference, ViewLike, ViewName,
        search_path::{
            expand_relation_search_path, expand_search_path_inst, expand_type_search_path,
        },
    },
};

/// schema information database
///
/// `BaseParseContext` is a Immutable view of schema information,
///
/// [`crate::context::ParseContext`] presents mutate functions.
///
pub trait BaseParseContext:
    crate::sem::RoleInfoRead
    + crate::sem::OwnershipInfoRead
    + CastInfoRead
    + ConflictInfoRead
    + FunctionInfoRead
    + crate::sem::ExtensionInfoRead
    + crate::sem::ObjectInfoRead
    + IndexInfoRead
    + OperatorInfoRead
    + RuleInfoRead
    + TriggerInfoRead
    + SequenceInfoRead
    + TypeInfoRead
    + TableInfoRead
    + crate::sem::ViewInsertInfoRead
    + crate::sem::ViewUpdateInfoRead
    + crate::sem::ViewDeleteInfoRead
    + crate::sem::ViewMergeInfoRead
    + ViewInfoRead
    + SearchPathRead
    + GetSourcePathWithSpanOpt
    + std::fmt::Debug
{
    /// Resolve an exact SQLite relation key using ASCII case-insensitive identifiers.
    fn sqlite_relation_identity(&self, name: &FullName) -> Option<FullName>;

    /// Resolved metadata for the database selected by the caller.
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata;

    /// get context args
    fn get_args(&self) -> &SqlFunArgs;

    /// get SQL file path
    fn get_source_file_path(&self) -> Option<&Path>;

    /// get type by name
    fn get_type(&self, key: &FullName) -> Option<&TypeDefinition> {
        expand_type_search_path(self, key, |k| self.get_type_impl(k))
    }

    /// Look up an index, expanding unqualified names through the active search path.
    /// Explicitly qualified names are read directly.
    fn get_index(&self, name: &FullName) -> Option<&CreateIndex> {
        expand_relation_search_path(self, name, |key| self.get_index_impl(key))
    }

    /// Resolve a type to the exact qualified catalog key selected by the search path.
    fn resolve_type_name(&self, name: &FullName) -> Option<FullName> {
        expand_type_search_path(self, name, |key| {
            self.get_type_impl(key)?;
            Some(key.clone())
        })
    }

    /// get `CREATE TABLE` statement by table name
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        expand_relation_search_path(self, name.full_name(), |key| self.get_table_impl(key))
    }

    /// Resolve a table to the qualified catalog key that matched the search path.
    /// A stored declaration can itself have an unqualified name.
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName> {
        expand_relation_search_path(self, name.full_name(), |key| {
            self.get_table_impl(key)?;
            Some(TableName::from(key.clone()))
        })
    }

    /// Resolve a relation key in search-path order across both tables and views.
    fn resolve_relation_name(&self, name: &FullName) -> Option<FullName> {
        expand_relation_search_path(self, name, |key| relation_name_at(self, key))
    }

    /// get `CREATE VIEW` statement by view name
    fn get_view(&self, key: &ViewName) -> Option<&ViewLike> {
        expand_relation_search_path(self, key.full_name(), |key| self.get_view_impl(key))
    }

    /// get `CREATE SEQUENCE` statement by sequence name
    fn get_sequence(&self, seq_name: &FullName) -> Option<&CreateSequence> {
        expand_relation_search_path(self, seq_name, |key| self.get_sequence_impl(key))
    }

    /// get `CREATE FUNCTION` statement by function name
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        expand_search_path_inst(self, name, |key| self.get_function_impl(key))
    }

    /// get resolved function name
    fn resolve_function_name_with_search_path(&self, fun_name: &FullName) -> FullName {
        if fun_name.schema().is_some() {
            return fun_name.clone();
        }

        expand_search_path_inst(self, fun_name, |key| {
            self.get_function_impl(key).map(|_| key.clone())
        })
        .unwrap_or_else(|| fun_name.clone())
    }

    /// Canonicalize a reference to the catalog key selected by the search path.
    fn canonicalize_type_ref(&self, type_ref: &TypeReference) -> Option<TypeReference> {
        let full_name = self.resolve_type_name(type_ref.full_name())?;
        Some(TypeReference::concrete_type_ref(
            &full_name,
            type_ref.is_array().unwrap_or(false),
        ))
    }
}

#[cfg(test)]
mod canonicalization_tests;

fn relation_name_at(
    context: &(impl BaseParseContext + ?Sized),
    key: &FullName,
) -> Option<FullName> {
    if context.get_table(&TableName::from(key.clone())).is_some()
        || context.get_view(&ViewName::from_full_name(key)).is_some()
    {
        Some(key.clone())
    } else {
        None
    }
}
