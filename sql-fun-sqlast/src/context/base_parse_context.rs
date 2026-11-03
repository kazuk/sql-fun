use crate::context::{
    ExtensionInfoRead, ObjectInfoRead, OwnershipInfoRead, RoleInfoRead, ViewDeleteInfoRead,
    ViewInsertInfoRead, ViewMergeInfoRead, ViewUpdateInfoRead,
};
use std::path::Path;

use ::sql_fun_core::SqlFunArgs;

use crate::{
    context::{
        CastInfoRead, ConflictInfoRead, FunctionInfoRead, GetSourcePathWithSpanOpt, IndexInfoRead,
        OperatorInfoRead, RuleInfoRead, SearchPathRead, SequenceInfoRead, TableInfoRead,
        TriggerInfoRead, TypeInfoRead, ViewInfoRead,
    },
    names::{FullName, TableName, ViewName},
    sem::{
        CreateIndex, CreateSequence, CreateTable, FunctionOverloadCollection, TypeDefinition,
        TypeReference, ViewLike,
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
    RoleInfoRead
    + OwnershipInfoRead
    + CastInfoRead
    + ConflictInfoRead
    + FunctionInfoRead
    + ExtensionInfoRead
    + ObjectInfoRead
    + IndexInfoRead
    + OperatorInfoRead
    + RuleInfoRead
    + TriggerInfoRead
    + SequenceInfoRead
    + TypeInfoRead
    + TableInfoRead
    + ViewInsertInfoRead
    + ViewUpdateInfoRead
    + ViewDeleteInfoRead
    + ViewMergeInfoRead
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

    /// Canonicalize a concrete reference through the search path; preserve owned definitions.
    fn canonicalize_type_ref(&self, type_ref: &TypeReference) -> Option<TypeReference> {
        if type_ref.is_dynamic() {
            return Some(type_ref.clone());
        }
        let full_name = self.resolve_type_name(type_ref.full_name()?)?;
        Some(TypeReference::concrete_type_ref(
            &full_name,
            type_ref.is_array().unwrap_or(false),
        ))
    }
}

#[cfg(test)]
mod canonicalization_tests {
    use crate::{sem::PgBuiltInType, test_helpers::dynamic_types::record};

    use crate::{
        context::{BaseParseContext, ParseContext},
        names::{FullName, SchemaName},
        sem::{TypeDefinition, TypeReference},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::first_schema(&["a", "b", "pg_catalog"], None, Some("a"))]
    #[case::reversed_path(&["b", "a", "pg_catalog"], None, Some("b"))]
    #[case::qualified_outside_path(&["a"], Some("b"), Some("b"))]
    #[case::empty_path(&[], None, Some("pg_catalog"))]
    #[case::implicit_catalog_first(&["a", "b"], None, Some("pg_catalog"))]
    #[case::explicit_catalog_last(&["a", "pg_catalog"], None, Some("a"))]
    #[case::missing_schema(&["a"], Some("absent"), None)]
    #[case::quoted_schema(&["a.b"], Some("a.b"), Some("a.b"))]
    fn canonicalization_retains_the_matched_key(
        #[case] path: &[&str],
        #[case] qualifier: Option<&str>,
        #[case] expected_schema: Option<&str>,
        #[values(false, true)] array: bool,
    ) {
        let mut context = TestParseContext::default();
        let schemas: Vec<_> = path.iter().map(|name| SchemaName::from(name)).collect();
        context.set_search_path(&schemas);
        let declaration = TypeDefinition::builtin(&FullName::from_local_name("item"), None);
        for schema in ["a", "b", "a.b", "pg_catalog"] {
            context.set_get_type_impl_result(&FullName::with_schema(schema, "item"), &declaration);
        }
        let name = match qualifier {
            Some(schema) => FullName::with_schema(schema, "item"),
            None => FullName::from_local_name("item"),
        };
        let expected = expected_schema.map(|schema| {
            TypeReference::concrete_type_ref(&FullName::with_schema(schema, "item"), array)
        });
        assert_eq!(
            context.canonicalize_type_ref(&TypeReference::concrete_type_ref(&name, array)),
            expected
        );
    }

    #[test]
    fn canonicalization_preserves_owned_anonymous_identity() {
        let context = TestParseContext::default();
        let row = record(&PgBuiltInType::int4());
        assert_eq!(context.canonicalize_type_ref(&row), Some(row.clone()));
        let array = row.array_type();
        assert_eq!(context.canonicalize_type_ref(&array), Some(array));
    }
}

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
