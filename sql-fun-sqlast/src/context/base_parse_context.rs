use crate::context::{
    ExtensionInfoRead, ObjectInfoRead, OwnershipInfoRead, RoleInfoRead, ViewDeleteInfoRead,
    ViewInsertInfoRead, ViewMergeInfoRead, ViewUpdateInfoRead,
};
use std::path::Path;

use ::sql_fun_core::SqlFunArgs;

use crate::{
    ast::ObjectKind,
    context::{
        AccessMethodInfoRead, CastInfoRead, ConflictInfoRead, ForeignDataWrapperInfoRead,
        FunctionInfoRead, GetSourcePathWithSpanOpt, IndexInfoRead, OperatorFamilyInfoRead,
        OperatorInfoRead, RuleInfoRead, SearchPathRead, SequenceInfoRead, TableInfoRead,
        TextSearchDictionaryInfoRead, TextSearchTemplateInfoRead, TransformationInfoRead,
        TriggerInfoRead, TypeInfoRead, ViewInfoRead,
    },
    names::{FullName, OperatorName, TableName, ViewName},
    sem::{
        CreateIndex, CreateSequence, CreateTable, ExactFunction, FunctionOverloadCollection,
        OperatorDefinition, TypeDefinition, TypeReference, ViewLike,
        search_path::{
            expand_operator_search_path, expand_relation_search_path, expand_search_path_inst,
            expand_type_search_path,
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
    + ForeignDataWrapperInfoRead
    + TransformationInfoRead
    + AccessMethodInfoRead
    + TextSearchTemplateInfoRead
    + TextSearchDictionaryInfoRead
    + OperatorFamilyInfoRead
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

    /// The selected `PostgreSQL` major and optional prerelease, when known.
    fn postgres_version(&self) -> Option<::sql_fun_core::PostgresVersion> {
        self.get_args()
            .postgres_version(self.metadata())
            .ok()?
            .to_string()
            .parse()
            .ok()
    }

    /// Whether another relation in the same namespace has this constraint name.
    fn constraint_name_exists_in_namespace(&self, target: &FullName, name: &str) -> bool {
        self.relation_names().into_iter().any(|relation| {
            &relation != target
                && relation.schema() == target.schema()
                && relation.catalog() == target.catalog()
                && self
                    .get_table_impl(&relation)
                    .is_some_and(|table| table.has_stored_constraint_name(name))
        })
    }

    /// get SQL file path
    fn get_source_file_path(&self) -> Option<&Path>;

    /// Look up an operator through the effective routine search path.
    /// Qualified names are looked up directly without selecting another schema.
    fn get_operator(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition> {
        expand_operator_search_path(self, name, |key| self.get_operator_op(key, left, right))
    }

    /// Retain the exact catalog identity selected for an operator reference.
    fn resolve_operator_name(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<OperatorName> {
        expand_operator_search_path(self, name, |key| {
            self.get_operator_op(key, left, right)?;
            Some(key.clone())
        })
    }

    /// Non-null result guarantee for the operator selected by name resolution.
    fn resolved_operator_result_is_not_null(&self, name: &OperatorName) -> Option<bool> {
        let name = self.resolve_operator_name(name, true, true)?;
        self.operator_result_is_not_null(&name)
    }

    /// get type by name
    fn get_type(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.get_declared_type(key)
            .filter(|definition| !definition.is_shell())
    }

    /// Look up a declared type, including an incomplete PostgreSQL shell.
    fn get_declared_type(&self, key: &FullName) -> Option<&TypeDefinition> {
        expand_type_search_path(self, key, |k| self.get_type_impl(k))
    }

    /// Look up an index, expanding unqualified names through the active search path.
    /// Explicitly qualified names are read directly.
    fn get_index(&self, name: &FullName) -> Option<&CreateIndex> {
        expand_relation_search_path(self, name, |key| self.get_index_impl(key))
    }

    /// Resolve a type to the exact qualified catalog key selected by the search path.
    fn resolve_type_name(&self, name: &FullName) -> Option<FullName> {
        self.resolve_declared_type_name(name, false)
    }

    /// Resolve a function signature's type, allowing a scalar shell when requested.
    fn resolve_declared_type_name(&self, name: &FullName, allow_shell: bool) -> Option<FullName> {
        expand_type_search_path(self, name, |key| {
            Some((key.clone(), self.get_type_impl(key)?))
        })
        .filter(|(_, definition)| allow_shell || !definition.is_shell())
        .map(|(key, _)| key)
    }

    /// Resolve a written signature type, including PostgreSQL array catalog names.
    fn resolve_signature_type(
        &self,
        name: &FullName,
        array: bool,
        allow_shell: bool,
    ) -> Option<TypeReference> {
        resolve_type_reference(self, name, array, allow_shell, true)
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

    /// Find the first visible overload with the exact input types.
    fn find_exact_function(
        &self,
        name: &FullName,
        arguments: &[Option<TypeReference>],
    ) -> Option<ExactFunction> {
        expand_search_path_inst(self, name, |key| {
            // Lookup must stop at an exact procedure even when expression
            // function lookup omits procedures from its overload collection.
            if self
                .ownership_definitions(ObjectKind::PROCEDURE, key)
                .iter()
                .any(|definition| definition.identity().arguments() == Some(arguments))
            {
                return Some(None);
            }
            self.get_function_impl(key)?
                .exact_function(key, arguments)
                .map(Some)
        })
        .flatten()
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
        let name = type_ref.full_name()?;
        let array = type_ref.is_array().unwrap_or(false);
        resolve_type_reference(self, name, array, false, false)
    }
}

fn resolve_type_reference(
    context: &(impl BaseParseContext + ?Sized),
    name: &FullName,
    array: bool,
    allow_shell: bool,
    require_array_type: bool,
) -> Option<TypeReference> {
    expand_type_search_path(context, name, |key| {
        if let Some(reference) = context.array_type_by_catalog_name(key) {
            return Some(if array { None } else { Some(reference) });
        }
        let definition = context.get_type_impl(key)?;
        let valid = if array && require_array_type {
            definition.has_postgres_array_type()
        } else {
            allow_shell || !definition.is_shell()
        };
        // An unusable type still shadows later schemas.
        Some(if valid {
            Some(TypeReference::concrete_type_ref(key, array))
        } else {
            None
        })
    })
    .flatten()
}

#[cfg(test)]
mod base_parse_context_tests {
    use crate::{sem::PgBuiltInType, test_helpers::dynamic_types::record};

    use crate::{
        context::{BaseParseContext, ParseContext},
        names::{FullName, OperatorName, SchemaName},
        sem::{TypeDefinition, TypeReference},
        test_helpers::{TestParseContext, operator_names::lookup_context},
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
    #[rstest::rstest]
    #[case(&[], None, Some("pg_catalog"))]
    #[case(&["a"], None, Some("pg_catalog"))]
    #[case(&["a", "pg_catalog"], None, Some("a"))]
    #[case(&["b", "a", "pg_catalog"], None, Some("b"))]
    #[case(&["a"], Some("b"), Some("b"))]
    #[case(&["a"], Some("a.b"), Some("a.b"))]
    #[case(&[], Some("Mixed"), Some("Mixed"))]
    #[case(&[], Some("mixed"), None)]
    #[case(&[], Some("PG_CATALOG"), None)]
    #[case(&[], Some("missing"), None)]
    fn operators_follow_routine_search_path(
        #[case] path: &[&str],
        #[case] schema: Option<&str>,
        #[case] expected: Option<&str>,
        #[values((true, true), (true, false), (false, true))] operands: (bool, bool),
    ) {
        let context = lookup_context(path, operands.0, operands.1);
        let name = OperatorName::from_parts(schema, "+");
        let resolved = context.resolve_operator_name(&name, operands.0, operands.1);
        assert_eq!(
            resolved,
            expected.map(|schema| OperatorName::from_parts(Some(schema), "+"))
        );
        assert_eq!(
            context
                .get_operator(&name, operands.0, operands.1)
                .is_some(),
            expected.is_some()
        );
        assert!(
            context
                .get_operator(&OperatorName::invalid(), operands.0, operands.1)
                .is_none()
        );
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
