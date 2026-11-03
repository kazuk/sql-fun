//! Semantic coverage for immutable schema layers and independent descendants.

// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;
use crate::test_helpers::heavy::schema_context_chain as heavy;
use crate::test_helpers::schema_context_chain;

use crate::test_helpers::schema_context_chain::{
    EXTENSION_A, EXTENSION_B, assert_missing_domain_base, assert_missing_relation,
    assert_primary_key, assert_query_type, assert_table_exists, assert_type_exists, dependent_dump,
    extension_sources, load_layer, load_schema, query,
};

use ::sql_fun_sqlast::{
    context::{
        BaseParseContext, GetSourcePathWithSpanOpt, IndexInfoRead, ParseContext, SearchPathRead,
        TypeInfoRead, ViewInfoRead,
    },
    names::{ColumnName, FullName, TableName},
    sem::{PgBuiltInType, TypeReference},
};

#[test]
fn anonymous_registration_returns_an_error_and_preserves_the_parent_catalog()
-> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::sem::{AnalysisError, CompositeFieldCollection, CreateCompositeType};
    let root = heavy::builtin_root()?;
    let before = root.type_references();
    let builder = schema_context_chain::schema_builder(&root)?;
    let definition = CreateCompositeType::from_fields(&CompositeFieldCollection::default());
    let error = builder.apply_create_comp_type(&definition).unwrap_err();
    assert!(matches!(error, AnalysisError::UnexpectedInput(_)));
    assert_eq!(root.type_references(), before);
    let named = load_schema(&root, "CREATE TYPE public.pair516 AS (n integer);")?;
    assert_type_exists(&named, "public.pair516");
    assert!(
        root.get_type(&FullName::with_schema("public", "pair516"))
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
#[case::no_extension(0, TypeReference::from_master_type_name("float8"))]
#[case::one_extension(1, PgBuiltInType::int4())]
#[case::two_extensions(2, PgBuiltInType::int4())]
fn runtime_file_chain_resolves_each_dump_from_its_ordered_prefix(
    #[case] extension_count: usize,
    #[case] answer_type: TypeReference,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut prefix = heavy::chain_prefix()?;
    for source in extension_sources(extension_count) {
        let loaded = load_schema(&prefix, source)?;
        assert!(loaded.problems().is_empty());
        prefix = loaded.base_context().clone();
    }
    let dump = load_schema(&prefix, dependent_dump(extension_count))?;

    assert!(dump.problems().is_empty());
    assert_table_exists(&dump, "public.base_items");
    let resolved = query(
        dump.base_context(),
        "SELECT answer FROM public.final_answers",
    )?;
    assert_query_type(&resolved, "answer", &answer_type);
    Ok(())
}

#[rstest::rstest]
#[case::unqualified("SELECT id FROM missing.items")]
#[case::qualified("SELECT m.id FROM missing.items AS m")]
fn missing_from_relation_reports_a_recoverable_problem(
    #[case] query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let parsed = load_schema(
        &root,
        &format!("CREATE VIEW public.missing_view AS {query};"),
    )?;

    assert_missing_relation(parsed.problems(), "missing.items");
    Ok(())
}

#[test]
fn dependent_view_keeps_missing_relation_problem_after_provider_is_loaded()
-> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let dependent = load_schema(
        &root,
        "CREATE VIEW public.missing_view AS SELECT id FROM missing.items;",
    )?;
    assert_missing_relation(dependent.problems(), "missing.items");

    let provider = load_schema(
        dependent.base_context(),
        "CREATE SCHEMA missing; CREATE TABLE missing.items (id integer);",
    )?;
    assert!(provider.problems().is_empty());
    assert_table_exists(&provider, "missing.items");
    assert_missing_relation(dependent.problems(), "missing.items");
    Ok(())
}

#[test]
fn dependent_file_reports_named_missing_domain_base_before_its_provider()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix = heavy::builtin_root()?;
    let dependent = load_schema(
        &prefix,
        "CREATE SCHEMA b; CREATE DOMAIN b.state_alias AS a.state;",
    )?;
    assert_missing_domain_base(dependent.problems(), "b.state_alias", "a.state");

    let provider = load_schema(dependent.base_context(), EXTENSION_A)?;
    assert!(provider.problems().is_empty());
    assert_type_exists(&provider, "a.state");
    // Rejected DDL is not published or retroactively repaired by a later file.
    let rejected = FullName::with_schema("b", "state_alias");
    assert!(dependent.get_type(&rejected).is_none());
    assert!(provider.get_type(&rejected).is_none());
    assert_missing_domain_base(dependent.problems(), "b.state_alias", "a.state");
    Ok(())
}

// Removed standalone whole_row_catalog.rs and the pg_class assertion here;
// restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[test]
fn inherited_catalog_objects_and_merged_overloads_drive_query_analysis()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = heavy::builtin_root()?;
    let first = load_schema(&builtin, EXTENSION_A)?;
    let second = load_schema(first.base_context(), EXTENSION_B)?;
    let dump = load_schema(second.base_context(), dependent_dump(2))?;

    assert!(first.problems().is_empty());
    assert!(second.problems().is_empty());
    assert!(dump.problems().is_empty());
    for name in [
        "a.state",
        "a.label",
        "a.pair",
        "a.items",
        "b.state_alias",
        "b.items",
        "public.final_state",
    ] {
        assert_type_exists(&dump, name);
    }
    let pair = dump
        .get_type(&FullName::with_schema("a", "pair"))
        .expect("composite type");
    assert!(pair.is_composite_type());
    assert_eq!(
        pair.get_composite_type_columns()
            .get_column_def(&ColumnName::from("key"))
            .expect("composite member")
            .get_type(),
        Some(&PgBuiltInType::int4())
    );
    let row_type = dump
        .get_type(&FullName::with_schema("a", "items"))
        .expect("table row type");
    assert_eq!(
        row_type
            .type_reference()
            .expect("named row reference")
            .full_name()
            .expect("nominal type identity"),
        &FullName::with_schema("a", "items")
    );
    assert!(
        dump.get_table(&TableName::from(FullName::with_schema("a", "items")))
            .is_some()
    );
    assert!(
        dump.get_table(&TableName::from(FullName::with_schema("b", "items")))
            .is_some()
    );
    let view = dump
        .get_view_impl(&FullName::with_schema("b", "answers"))
        .expect("inherited ordinary view");
    assert!(!view.is_materialized());
    assert_eq!(
        view.get_column_def(&ColumnName::from("answer"))
            .expect("view column")
            .get_type(),
        Some(&PgBuiltInType::int4())
    );
    assert!(
        dump.get_view_impl(&FullName::with_schema("b", "cached_answers"))
            .expect("materialized view")
            .is_materialized()
    );
    assert_eq!(
        dump.get_sequence(&FullName::with_schema("a", "item_seq"))
            .expect("sequence")
            .name(),
        &FullName::with_schema("a", "item_seq")
    );
    assert_eq!(
        dump.get_index(&FullName::with_schema("a", "items_id_idx"))
            .expect("inherited index")
            .name(),
        &FullName::with_schema("a", "items_id_idx")
    );
    assert_eq!(
        dump.get_function_by_name(&FullName::with_schema("a", "choose"))
            .expect("merged overloads")
            .len(),
        2
    );

    let resolved = query(
        dump.base_context(),
        "SELECT a.choose(1) AS number, a.choose(CAST('x' AS text)) AS label, \
         abs(CAST(1 AS double precision)) AS builtin_number, 1 + 1 AS builtin_sum, \
         CAST(1 AS bigint) AS widened",
    )?;
    assert_query_type(&resolved, "number", &PgBuiltInType::int4());
    assert_query_type(&resolved, "label", &PgBuiltInType::text());
    assert_query_type(
        &resolved,
        "builtin_number",
        &TypeReference::from_master_type_name("float8"),
    );
    assert_query_type(&resolved, "builtin_sum", &PgBuiltInType::int4());
    assert_query_type(&resolved, "widened", &PgBuiltInType::int8());
    Ok(())
}

#[rstest::rstest]
#[case::scalar(false, None)]
#[case::array(true, Some(PgBuiltInType::int4()))]
fn array_item_reads_agree_across_context_layers(
    #[case] is_array: bool,
    #[case] expected: Option<TypeReference>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_context()?;
    let reference =
        TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), is_array);
    assert_eq!(root.get_array_item_type(&reference), expected);
    let builtin = ::sql_fun_sqlast::context::FrozenBaseContext::from(root);
    assert_eq!(builtin.get_array_item_type(&reference), expected);

    let dump = load_schema(&builtin, "CREATE TABLE public.items (id integer);")?;
    assert_eq!(
        dump.base_context().get_array_item_type(&reference),
        expected
    );
    let builder = schema_context_chain::schema_builder(dump.base_context())?;
    assert_eq!(builder.get_array_item_type(&reference), expected);
    assert_eq!(builder.freeze().get_array_item_type(&reference), expected);
    Ok(())
}

#[rstest::rstest]
#[case::integer("integer[]")]
#[case::qualified("pg_catalog.int4[]")]
#[case::bounded("integer[3][4]")]
fn sql_array_declarations_survive_schema_layers(
    #[case] declaration: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let parsed = load_schema(
        &root,
        &format!(
            "CREATE TABLE public.array_items (values {declaration}, scalar integer); \
             CREATE VIEW public.array_view AS \
             SELECT values[1] AS n, values AS items, scalar FROM public.array_items;"
        ),
    )?;
    assert!(parsed.problems().is_empty(), "{:?}", parsed.problems());
    let array =
        TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), true);
    let table = parsed
        .get_table(&TableName::from(FullName::with_schema(
            "public",
            "array_items",
        )))
        .expect("SQL-declared table");
    assert_eq!(
        table
            .get_column_def(&ColumnName::from("values"))
            .expect("array column")
            .get_type(),
        Some(&array)
    );
    assert_eq!(
        table
            .get_column_def(&ColumnName::from("scalar"))
            .expect("scalar column")
            .get_type(),
        Some(&PgBuiltInType::int4())
    );
    let view = parsed
        .get_view_impl(&FullName::with_schema("public", "array_view"))
        .expect("SQL-declared view");
    assert_query_type(view, "n", &PgBuiltInType::int4());
    assert_query_type(view, "items", &array);
    assert_query_type(view, "scalar", &PgBuiltInType::int4());

    let descendant = query(
        parsed.base_context(),
        "SELECT values[1] AS n, values AS items, scalar FROM public.array_items",
    )?;
    assert_query_type(&descendant, "n", &PgBuiltInType::int4());
    assert_query_type(&descendant, "items", &array);
    assert_query_type(&descendant, "scalar", &PgBuiltInType::int4());
    Ok(())
}

#[test]
fn sql_scalar_subscript_reports_array_required() -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let parsed = load_schema(
        &root,
        "CREATE TABLE public.scalar_items (value integer); \
         CREATE VIEW public.invalid_array_view AS SELECT value[1] FROM public.scalar_items;",
    )?;
    assert_eq!(parsed.problems().len(), 1, "{:?}", parsed.problems());
    assert_eq!(parsed.problems()[0].code(), "require_array");
    Ok(())
}

#[test]
fn tablefunc_crosstab_call_resolves_after_explicit_extension_loading()
-> Result<(), Box<dyn std::error::Error>> {
    let extension = heavy::tablefunc_schema()?;
    assert_eq!(
        extension
            .get_function_by_name(&FullName::with_schema("public", "crosstab"))
            .expect("loaded crosstab overloads")
            .len(),
        3
    );
    let resolved = query(
        &extension,
        "SELECT result.row_name, result.value FROM public.crosstab('SELECT 1, 2, 3') \
         AS result(row_name text, value integer)",
    )?;
    assert_query_type(&resolved, "row_name", &PgBuiltInType::text());
    assert_query_type(&resolved, "value", &PgBuiltInType::int4());
    let composite_call = query(
        &extension,
        "SELECT public.crosstab2('SELECT 1, 2, 3') AS row_value",
    )?;
    let composite = extension
        .get_type(&FullName::with_schema("public", "tablefunc_crosstab_2"))
        .expect("visible tablefunc composite");
    assert!(composite.is_composite_type());
    assert_query_type(
        &composite_call,
        "row_value",
        composite
            .type_reference()
            .expect("named composite reference"),
    );
    let returned_type = composite_call
        .get_column_def(&ColumnName::from("row_value"))
        .expect("composite result column")
        .get_type_cloned()
        .expect("composite result type");
    assert!(
        extension
            .get_type(returned_type.full_name().expect("nominal type identity"))
            .expect("resolved composite result type")
            .is_composite_type()
    );
    Ok(())
}

#[test]
fn cloned_prefix_outlives_its_parse_result_and_supports_shared_thread_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = heavy::builtin_root()?;
    let parsed = load_schema(&builtin, EXTENSION_A)?;
    let prefix = parsed.base_context().clone();
    drop(parsed);
    let mut readers = Vec::new();
    for _ in 0..3 {
        let shared = prefix.clone();
        readers.push(std::thread::spawn(move || {
            assert_type_exists(&shared, "a.label");
            assert!(
                shared
                    .get_table(&TableName::from(FullName::with_schema("a", "items")))
                    .is_some()
            );
            assert_eq!(
                shared
                    .get_function_by_name(&FullName::with_schema("a", "choose"))
                    .expect("shared function")
                    .len(),
                1
            );
        }));
    }
    drop(prefix);
    for reader in readers {
        reader.join().expect("shared catalog reader");
    }
    Ok(())
}

#[test]
fn forked_files_keep_definitions_search_paths_provenance_and_problems_local()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = heavy::builtin_root()?;
    let parent = load_schema(&builtin, EXTENSION_A)?;
    let left = load_layer(
        parent.base_context(),
        &["a"],
        "CREATE TABLE a.left_only (id integer); CREATE DOMAIN a.broken AS missing.base;",
    )?;
    let right = load_layer(
        parent.base_context(),
        &["b"],
        "CREATE SCHEMA b; CREATE TABLE b.right_only (id integer);",
    )?;
    let left_name = TableName::from(FullName::with_schema("a", "left_only"));
    let right_name = TableName::from(FullName::with_schema("b", "right_only"));

    assert!(left.get_table(&left_name).is_some());
    assert!(left.get_table(&right_name).is_none());
    assert!(right.get_table(&right_name).is_some());
    assert!(right.get_table(&left_name).is_none());
    assert!(parent.get_table(&left_name).is_none());
    assert!(parent.get_table(&right_name).is_none());
    assert_missing_domain_base(left.problems(), "a.broken", "missing.base");
    assert!(right.problems().is_empty());
    assert!(parent.problems().is_empty());
    assert_eq!(left.get_default_schema().expect("left path").as_str(), "a");
    assert_eq!(
        right.get_default_schema().expect("right path").as_str(),
        "b"
    );
    assert_eq!(
        parent.get_default_schema().expect("parent path").as_str(),
        "public"
    );
    let left_source = left.get_source_path_with_span().expect("left provenance");
    let right_source = right.get_source_path_with_span().expect("right provenance");
    let parent_source = parent
        .get_source_path_with_span()
        .expect("parent provenance");
    assert_ne!(left_source.source_path(), right_source.source_path());
    assert_ne!(left_source.source_path(), parent_source.source_path());
    assert_ne!(right_source.source_path(), parent_source.source_path());
    assert_eq!(left.get_source_file_path(), Some(left_source.source_path()));
    assert_eq!(
        right.get_source_file_path(),
        Some(right_source.source_path())
    );
    Ok(())
}

#[rstest::rstest]
#[case::unqualified_first("items", &["a", "b"], "a", "b", Some("other_id"))]
#[case::unqualified_reversed("items", &["b", "a"], "b", "a", Some("other_id"))]
#[case::qualified_a("a.items", &["b", "a"], "a", "b", None)]
#[case::qualified_b("b.items", &["a", "b"], "b", "a", None)]
fn inherited_alter_updates_only_the_resolved_child_table(
    #[case] target: &str,
    #[case] search_path: &[&str],
    #[case] selected_schema: &str,
    #[case] other_schema: &str,
    #[case] unqualified_key: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = heavy::parent_with_same_named_tables(false)?;
    let alteration = format!(
        "ALTER TABLE {target} ADD CONSTRAINT items_pk PRIMARY KEY (other_id); \
         SELECT id, other_id FROM {target};"
    );
    let child = load_layer(parent.base_context(), search_path, &alteration)?;
    let sibling = load_layer(
        parent.base_context(),
        search_path,
        &format!(
            "ALTER TABLE {selected_schema}.items ADD CONSTRAINT items_pk PRIMARY KEY (sibling_id);"
        ),
    )?;

    assert!(child.problems().is_empty());
    assert!(sibling.problems().is_empty());
    assert_primary_key(
        &child,
        &format!("{selected_schema}.items"),
        Some("other_id"),
    );
    assert_primary_key(&child, "items", unqualified_key);
    assert_primary_key(&child, &format!("{other_schema}.items"), None);
    assert_primary_key(
        &sibling,
        &format!("{selected_schema}.items"),
        Some("sibling_id"),
    );
    assert_primary_key(&parent, "a.items", None);
    assert_primary_key(&parent, "b.items", None);
    Ok(())
}

#[rstest::rstest]
#[case::qualified_a("a.items", &["b", "a"], "a", "b")]
#[case::qualified_b("b.items", &["a", "b"], "b", "a")]
#[case::unqualified_first("items", &["a", "b"], "a", "b")]
#[case::unqualified_reversed("items", &["b", "a"], "b", "a")]
fn inherited_unqualified_declarations_are_altered_under_the_resolved_key(
    #[case] target: &str,
    #[case] search_path: &[&str],
    #[case] selected_schema: &str,
    #[case] other_schema: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let first = load_layer(
        &root,
        &["a"],
        "CREATE SCHEMA a; CREATE TABLE items (id integer);",
    )?;
    let parent = load_layer(
        first.base_context(),
        &["b"],
        "CREATE SCHEMA b; CREATE TABLE items (id integer);",
    )?;
    let child = load_layer(
        parent.base_context(),
        search_path,
        &format!("ALTER TABLE {target} ADD CONSTRAINT items_pk PRIMARY KEY (id);"),
    )?;
    let sibling = load_layer(parent.base_context(), search_path, "SELECT 1;")?;

    assert!(child.problems().is_empty());
    assert_primary_key(&child, &format!("{selected_schema}.items"), Some("id"));
    assert_primary_key(&child, &format!("{other_schema}.items"), None);
    assert_primary_key(&parent, "a.items", None);
    assert_primary_key(&parent, "b.items", None);
    assert_primary_key(&sibling, "a.items", None);
    assert_primary_key(&sibling, "b.items", None);
    Ok(())
}

#[rstest::rstest]
#[case::qualified("public.items")]
#[case::unqualified("items")]
fn local_unqualified_declaration_retains_its_catalog_key_after_alter(
    #[case] target: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::builtin_root()?;
    let parsed = load_schema(
        &root,
        &format!(
            "CREATE TABLE items (id integer); ALTER TABLE {target} ADD CONSTRAINT items_pk PRIMARY KEY (id);"
        ),
    )?;

    assert!(parsed.problems().is_empty());
    assert_primary_key(&parsed, "public.items", Some("id"));
    assert_primary_key(&parsed, "items", Some("id"));
    Ok(())
}

#[rstest::rstest]
#[case::unqualified_first("items", &["a", "b"], "a")]
#[case::unqualified_reversed("items", &["b", "a"], "b")]
#[case::qualified_a("a.items", &["b", "a"], "a")]
#[case::qualified_b("b.items", &["a", "b"], "b")]
fn inherited_drop_records_without_removing_tables_or_changing_other_layers(
    #[case] target: &str,
    #[case] search_path: &[&str],
    #[case] selected_schema: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = heavy::parent_with_same_named_tables(true)?;
    let child = load_layer(
        parent.base_context(),
        search_path,
        &format!("DROP TABLE {target}; SELECT id FROM {target};"),
    )?;
    let sibling = load_layer(
        parent.base_context(),
        search_path,
        &format!("SELECT id FROM {selected_schema}.items;"),
    )?;

    assert!(child.problems().is_empty());
    assert!(sibling.problems().is_empty());
    assert_primary_key(&child, "items", Some("id"));
    assert_primary_key(&child, "a.items", Some("id"));
    assert_primary_key(&child, "b.items", Some("id"));
    assert_primary_key(&sibling, "a.items", Some("id"));
    assert_primary_key(&sibling, "b.items", Some("id"));
    assert_primary_key(&parent, "a.items", Some("id"));
    assert_primary_key(&parent, "b.items", Some("id"));
    Ok(())
}

#[test]
fn missing_index_table_recovers_without_registering_and_analyzes_later_statements()
-> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::index_root()?;
    let parsed = load_schema(
        &root,
        r"
        CREATE INDEX missing_idx ON missing.items(id);
        CREATE TABLE public.items(id integer);
        CREATE INDEX valid_idx ON public.items(id);
        CREATE VIEW public.missing_view AS SELECT 1 AS id FROM later_missing.items;
    ",
    )?;
    assert_missing_relation(parsed.problems(), "missing.items");
    assert_missing_relation(parsed.problems(), "later_missing.items");
    assert_eq!(parsed.problems().len(), 2);
    assert_table_exists(&parsed, "public.items");
    let valid = FullName::with_schema("public", "valid_idx");
    assert_eq!(
        parsed.get_index(&valid).expect("later valid index").name(),
        &valid
    );
    assert!(
        parsed
            .get_index_impl(&FullName::with_schema("missing", "missing_idx"))
            .is_none()
    );
    assert!(
        parsed
            .get_index(&FullName::from_local_name("missing_idx"))
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
#[case::qualified("CREATE INDEX idx ON a.items(id)", "a")]
#[case::unqualified("CREATE INDEX idx ON items(id)", "a")]
fn index_creation_uses_resolved_table_schema(
    #[case] statement: &str,
    #[case] schema: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::index_root()?;
    let tables = load_schema(
        &root,
        "CREATE SCHEMA a; CREATE SCHEMA b; CREATE TABLE a.items(id integer);",
    )?;
    let indexes = load_layer(tables.base_context(), &["b", "a"], statement)?;
    assert!(indexes.problems().is_empty());
    let name = FullName::with_schema(schema, "idx");
    assert_eq!(
        indexes.get_index(&name).expect("resolved index").name(),
        &name
    );
    assert!(
        indexes
            .get_index(&FullName::with_schema("b", "idx"))
            .is_none()
    );
    Ok(())
}

#[test]
fn index_identity_uses_catalog_key_of_unqualified_table_declaration()
-> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::index_root()?;
    let tables = load_layer(&root, &["a"], "CREATE TABLE items(id integer);")?;
    let indexes = load_layer(
        tables.base_context(),
        &["b", "a"],
        "CREATE INDEX idx ON items(id); ALTER TABLE items ADD CONSTRAINT pk PRIMARY KEY(id);",
    )?;
    assert!(indexes.problems().is_empty());
    for local in ["idx", "pk"] {
        let name = FullName::with_schema("a", local);
        assert_eq!(
            indexes.get_index(&name).expect("resolved index").name(),
            &name
        );
    }
    Ok(())
}

#[test]
fn explicit_and_constraint_indexes_in_distinct_schemas_coexist()
-> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::index_root()?;
    let parsed = load_schema(
        &root,
        r"
        CREATE SCHEMA a; CREATE SCHEMA b;
        CREATE TABLE a.items(id integer, value integer);
        ALTER TABLE a.items ADD CONSTRAINT pk PRIMARY KEY(id);
        ALTER TABLE a.items ADD CONSTRAINT uq UNIQUE(value);
        CREATE TABLE b.items(id integer, value integer);
        ALTER TABLE b.items ADD CONSTRAINT pk PRIMARY KEY(id);
        ALTER TABLE b.items ADD CONSTRAINT uq UNIQUE(value);
        CREATE INDEX idx ON a.items(id);
        CREATE INDEX idx ON b.items(id);
    ",
    )?;
    assert!(parsed.problems().is_empty());
    for schema in ["a", "b"] {
        for local in ["idx", "pk", "uq"] {
            let name = FullName::with_schema(schema, local);
            assert_eq!(
                parsed.get_index(&name).expect("distinct index").name(),
                &name
            );
        }
    }
    Ok(())
}

#[rstest::rstest]
#[case::own_schema("CREATE INDEX idx ON a.items(id);", false)]
#[case::other_schema("CREATE INDEX idx ON b.items(id);", true)]
fn cluster_index_lookup_uses_target_table_schema(
    #[case] create_index: &str,
    #[case] missing: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = heavy::index_root()?;
    let tables = load_schema(
        &root,
        "CREATE SCHEMA a; CREATE SCHEMA b; CREATE TABLE a.items(id integer); CREATE TABLE b.items(id integer);",
    )?;
    let indexes = load_schema(tables.base_context(), create_index)?;
    let clustered = load_layer(
        indexes.base_context(),
        &["b"],
        "ALTER TABLE a.items CLUSTER ON idx;",
    )?;
    assert_eq!(!clustered.problems().is_empty(), missing);
    Ok(())
}

#[test]
fn absent_index_traverses_schema_layers_to_builtin_root() -> Result<(), Box<dyn std::error::Error>>
{
    let root = heavy::index_root()?;
    let missing = FullName::with_schema("public", "absent");
    assert!(root.get_index_impl(&missing).is_none());
    let parent = load_schema(&root, "CREATE TABLE public.items(id integer);")?;
    let child = load_schema(
        parent.base_context(),
        "CREATE INDEX idx ON public.items(id);",
    )?;
    assert!(child.get_index(&missing).is_none());
    assert!(
        child
            .get_index(&FullName::from_local_name("absent"))
            .is_none()
    );
    Ok(())
}
