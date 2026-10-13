//! Semantic coverage for immutable schema layers and independent descendants.

use self::test_helpers::{
    EXTENSION_A, EXTENSION_B, assert_missing_domain_base, assert_missing_relation,
    assert_primary_key, assert_query_type, assert_table_exists, assert_type_exists, builtin_root,
    chain_prefix, dependent_dump, extension_sources, heavy, load_layer, load_schema,
    parent_with_same_named_tables, query,
};

use ::sql_fun_sqlast::sem::{
    BaseParseContext, ColumnName, FullName, GetSourcePathWithSpanOpt, IndexInfoRead, PgBuiltInType,
    SearchPathRead, TableName, TypeInfoRead, TypeReference, ViewInfoRead,
};

#[rstest::rstest]
#[case::no_extension(0, TypeReference::from_master_type_name("float8"))]
#[case::one_extension(1, PgBuiltInType::int4())]
#[case::two_extensions(2, PgBuiltInType::int4())]
fn runtime_file_chain_resolves_each_dump_from_its_ordered_prefix(
    #[case] extension_count: usize,
    #[case] answer_type: TypeReference,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut prefix = chain_prefix()?;
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
    let root = builtin_root()?;
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
    let root = builtin_root()?;
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
    let prefix = builtin_root()?;
    let dependent = load_schema(
        &prefix,
        "CREATE SCHEMA b; CREATE DOMAIN b.state_alias AS a.state;",
    )?;
    assert_missing_domain_base(dependent.problems(), "b.state_alias", "a.state");

    let provider = load_schema(dependent.base_context(), EXTENSION_A)?;
    assert!(provider.problems().is_empty());
    assert_type_exists(&provider, "a.state");
    assert_type_exists(&provider, "b.state_alias");
    assert_missing_domain_base(dependent.problems(), "b.state_alias", "a.state");
    Ok(())
}

#[test]
fn inherited_catalog_objects_and_merged_overloads_drive_query_analysis()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = heavy::builtin_schema()?;
    let first = load_schema(&builtin, EXTENSION_A)?;
    let second = load_schema(first.base_context(), EXTENSION_B)?;
    let dump = load_schema(second.base_context(), dependent_dump(2))?;

    assert!(first.problems().is_empty());
    assert!(second.problems().is_empty());
    assert!(dump.problems().is_empty());
    assert_table_exists(&dump, "pg_catalog.pg_class");
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
            .full_name(),
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

#[test]
fn array_item_reads_follow_frozen_and_mutable_schema_layers()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = builtin_root()?;
    let first = load_schema(&builtin, EXTENSION_A)?;
    let second = load_schema(first.base_context(), EXTENSION_B)?;
    let dump = load_schema(second.base_context(), dependent_dump(2))?;

    let array =
        TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), true);
    assert_eq!(
        dump.base_context().get_array_item_type(&array),
        Some(PgBuiltInType::int4())
    );
    let builder = ::sql_fun_sqlast::sem::SchemaFileContext::new(
        std::path::Path::new("array.sql"),
        dump.base_context(),
    );
    assert_eq!(
        builder.get_array_item_type(&array),
        Some(PgBuiltInType::int4())
    );
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
        "SELECT 1 AS n FROM public.crosstab('SELECT 1, 2, 3') \
         AS result(row_name text, value integer)",
    )?;
    assert_query_type(&resolved, "n", &PgBuiltInType::int4());
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
            .get_type(returned_type.full_name())
            .expect("resolved composite result type")
            .is_composite_type()
    );
    Ok(())
}

#[test]
fn cloned_prefix_outlives_its_parse_result_and_supports_shared_thread_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let builtin = builtin_root()?;
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
    let builtin = builtin_root()?;
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
    let parent = parent_with_same_named_tables(false)?;
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
    let root = builtin_root()?;
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
    let root = builtin_root()?;
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
    let parent = parent_with_same_named_tables(true)?;
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
        r#"
        CREATE INDEX missing_idx ON missing.items(id);
        CREATE TABLE public.items(id integer);
        CREATE INDEX valid_idx ON public.items(id);
        CREATE VIEW public.missing_view AS SELECT 1 AS id FROM later_missing.items;
    "#,
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
        r#"
        CREATE SCHEMA a; CREATE SCHEMA b;
        CREATE TABLE a.items(id integer, value integer);
        ALTER TABLE a.items ADD CONSTRAINT pk PRIMARY KEY(id);
        ALTER TABLE a.items ADD CONSTRAINT uq UNIQUE(value);
        CREATE TABLE b.items(id integer, value integer);
        ALTER TABLE b.items ADD CONSTRAINT pk PRIMARY KEY(id);
        ALTER TABLE b.items ADD CONSTRAINT uq UNIQUE(value);
        CREATE INDEX idx ON a.items(id);
        CREATE INDEX idx ON b.items(id);
    "#,
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

mod test_helpers {
    pub(super) mod heavy {
        use std::path::Path;

        use ::sql_fun_sqlast::{PostgresDialect, sem::FrozenBaseContext};

        use super::builtin_root;

        /// Load real builtin type/function metadata for index absence and DDL
        /// resolution tests. Every call reloads metadata, without sharing across
        /// calls or nextest processes; the full builtin schema is not parsed.
        pub fn index_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            builtin_root()
        }

        /// Parse the complete PostgreSQL 17 builtin schema for catalog integration
        /// coverage. Every call reloads metadata and parses the schema; results
        /// are not cached or shared between calls or test processes.
        pub fn builtin_schema() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            let root = builtin_root()?;
            let file = Path::new(env!("SQL_FUN_HOME")).join("postgres/17/schema.sql");
            Ok(PostgresDialect::parse_schema_file(&file, &root, &mut None)?
                .base_context()
                .clone())
        }

        /// Parse the complete builtin schema and tablefunc extension for real
        /// extension integration coverage. Every call repeats both schema loads;
        /// results are not cached or shared between calls or test processes.
        pub fn tablefunc_schema() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            let prefix = builtin_schema()?;
            let file =
                Path::new(env!("SQL_FUN_HOME")).join("postgres/17/extension/tablefunc--1.0.sql");
            Ok(
                PostgresDialect::parse_schema_file(&file, &prefix, &mut None)?
                    .base_context()
                    .clone(),
            )
        }
    }

    use std::path::Path;

    use ::clap::Parser;
    use ::sql_fun_sqlast::{
        ParsedSchemaDump, PostgresDialect, SqlFunArgs,
        sem::{
            AnalysisProblem, BaseContext, BaseParseContext, ColumnName, FrozenBaseContext,
            FullName, ParseContext, SchemaFileContext, SchemaName, TableName, TypeReference,
            ViewInfoRead, ViewLike, problem::NameResolution,
        },
    };

    pub(super) const EXTENSION_A: &str = "CREATE SCHEMA a; CREATE TYPE a.state AS ENUM ('ready'); CREATE DOMAIN a.label AS text; \
         CREATE TYPE a.pair AS (key integer, value text); \
         CREATE TABLE a.items (id integer, label a.label); \
         CREATE FUNCTION a.choose(x integer) RETURNS integer LANGUAGE sql STRICT AS 'SELECT x'; \
         CREATE VIEW a.answers AS SELECT a.choose(t.id) AS answer, t.label FROM a.items t; \
         CREATE SEQUENCE a.item_seq; CREATE INDEX items_id_idx ON a.items (id);";

    pub(super) const EXTENSION_B: &str = "CREATE SCHEMA b; CREATE DOMAIN b.state_alias AS a.state; CREATE DOMAIN b.label_alias AS a.label; \
         CREATE TABLE b.items (id integer, label a.label, state b.state_alias); \
         CREATE FUNCTION a.choose(x text) RETURNS text LANGUAGE sql STRICT AS 'SELECT x'; \
         CREATE VIEW b.answers AS SELECT a.choose(t.id) AS answer, t.label FROM a.items t; \
         CREATE MATERIALIZED VIEW b.cached_answers AS SELECT answer FROM b.answers; \
         CREATE EXTENSION tablefunc;";

    pub(super) fn extension_sources(count: usize) -> &'static [&'static str] {
        &[EXTENSION_A, EXTENSION_B][..count]
    }

    pub(super) fn dependent_dump(count: usize) -> &'static str {
        match count {
            0 => {
                "CREATE TABLE public.final_items (id integer); \
                  CREATE VIEW public.final_answers AS SELECT abs(CAST(id AS double precision)) AS answer FROM public.final_items;"
            }
            1 => {
                "CREATE DOMAIN public.final_label AS a.label; \
                  CREATE VIEW public.final_answers AS SELECT answer, label FROM a.answers;"
            }
            2 => {
                "CREATE DOMAIN public.final_state AS b.state_alias; \
                  CREATE VIEW public.final_answers AS SELECT answer, label FROM b.answers;"
            }
            _ => panic!("unsupported fixture extension count"),
        }
    }

    pub(super) fn parent_with_same_named_tables(
        with_primary_keys: bool,
    ) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
        let root = builtin_root()?;
        let mut schema = String::from(
            "CREATE SCHEMA a; CREATE SCHEMA b; \
             CREATE TABLE a.items (id INTEGER, other_id INTEGER, sibling_id INTEGER); \
             CREATE TABLE b.items (id INTEGER, other_id INTEGER, sibling_id INTEGER);",
        );
        if with_primary_keys {
            schema.push_str(
                "ALTER TABLE a.items ADD CONSTRAINT items_pk PRIMARY KEY (id); \
                 ALTER TABLE b.items ADD CONSTRAINT items_pk PRIMARY KEY (id);",
            );
        }
        load_layer(&root, &["a", "b"], &schema)
    }

    pub(super) fn builtin_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
        // Real type/function/operator/cast metadata is sufficient for small fixtures;
        // only catalog and full-extension tests need the entire builtin schema SQL.
        let metadata = Path::new(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
        let args = SqlFunArgs::try_parse_from([
            "sqlfun",
            "--metadata-file",
            metadata.to_str().expect("fixture metadata path"),
            "--sql-fun-home",
            env!("SQL_FUN_HOME"),
            "--postgres-version",
            "17",
        ])?;
        let metadata = args.load_metadata()?;
        let selected = metadata.get("default").expect("selected fixture metadata");
        Ok(FrozenBaseContext::from(BaseContext::new(&args, selected)?))
    }

    pub(super) fn chain_prefix() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
        let root = builtin_root()?;
        let parsed = load_schema(&root, "CREATE TABLE public.base_items (id integer);")?;
        assert!(parsed.problems().is_empty());
        Ok(parsed.base_context().clone())
    }

    pub(super) fn load_schema(
        base: &FrozenBaseContext,
        sql: &str,
    ) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
        let file = ::tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), sql)?;
        Ok(PostgresDialect::parse_schema_file(
            file.path(),
            base,
            &mut None,
        )?)
    }

    pub(super) fn query(
        base: &FrozenBaseContext,
        sql: &str,
    ) -> Result<ViewLike, Box<dyn std::error::Error>> {
        let parsed = load_schema(
            base,
            &format!("CREATE VIEW public.acceptance_query AS {sql};"),
        )?;
        assert!(parsed.problems().is_empty());
        Ok(parsed
            .get_view_impl(&FullName::with_schema("public", "acceptance_query"))
            .expect("analyzed query view")
            .clone())
    }

    pub(super) fn assert_query_type(query: &ViewLike, column: &str, expected: &TypeReference) {
        assert_eq!(
            query
                .get_column_def(&ColumnName::from(column))
                .expect("resolved query column")
                .get_type(),
            Some(expected)
        );
    }

    pub(super) fn assert_type_exists(context: &impl BaseParseContext, name: &str) {
        let full_name = name.parse::<FullName>().expect("fixture type name");
        assert_eq!(
            context
                .get_type(&full_name)
                .expect("visible named type")
                .full_name(),
            Some(&full_name)
        );
    }

    pub(super) fn assert_table_exists(context: &impl BaseParseContext, name: &str) {
        let table_name = TableName::from(name.parse::<FullName>().expect("fixture table name"));
        assert!(context.get_table(&table_name).is_some());
    }

    pub(super) fn assert_missing_domain_base(
        problems: &[AnalysisProblem],
        domain: &str,
        base: &str,
    ) {
        let domain_name = domain.parse::<FullName>().expect("fixture domain name");
        assert!(problems.iter().any(|problem| match problem {
            AnalysisProblem::NameResolution(NameResolution::DomainTypeNameNotFound(problem)) =>
                problem.domain_type_name() == &domain_name && problem.base_type_name() == base,
            _ => false,
        }));
    }

    pub(super) fn assert_missing_relation(problems: &[AnalysisProblem], relation: &str) {
        assert!(problems.iter().any(|problem| matches!(
            problem,
            AnalysisProblem::NameResolution(NameResolution::RelationNotFound(name))
                if name == relation
        )));
    }

    pub(super) fn load_layer(
        base: &FrozenBaseContext,
        search_path: &[&str],
        sql: &str,
    ) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
        let file = ::tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), sql)?;
        let mut context = SchemaFileContext::new(file.path(), base);
        let mut schemas = Vec::new();
        for schema in search_path {
            schemas.push(SchemaName::from(schema));
        }
        context.set_search_path(&schemas);
        Ok(PostgresDialect::parse_schema_file(
            file.path(),
            &context.freeze(),
            &mut None,
        )?)
    }

    pub(super) fn assert_primary_key(
        context: &impl BaseParseContext,
        table: &str,
        key: Option<&str>,
    ) {
        let table_name = TableName::from(table.parse::<FullName>().expect("fixture table name"));
        let table = context.get_table(&table_name).expect("resolved table");
        if let Some(key) = key {
            let expected = ColumnName::from(key);
            assert_eq!(table.primary_key_columns(), vec![&expected]);
        } else {
            assert_eq!(table.primary_key_columns(), Vec::<&ColumnName>::new());
        }
    }
}
