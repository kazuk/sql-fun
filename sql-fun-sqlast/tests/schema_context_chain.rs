//! Semantic coverage for immutable schema layers and independent descendants.

use self::test_helpers::{assert_primary_key, load_layer, parent_with_same_named_tables};

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

mod test_helpers {
    use std::path::Path;

    use ::clap::Parser;
    use ::sql_fun_sqlast::{
        ParsedSchemaDump, PostgresDialect, SqlFunArgs,
        sem::{
            BaseContext, BaseParseContext, ColumnName, FrozenBaseContext, FullName, ParseContext,
            SchemaFileContext, SchemaName, TableName,
        },
    };

    pub(super) fn parent_with_same_named_tables(
        with_primary_keys: bool,
    ) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
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
        let root = FrozenBaseContext::from(BaseContext::new(&args, selected)?);
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
            schemas.push(SchemaName::from(*schema));
        }
        context.set_search_path(&schemas);
        Ok(PostgresDialect::parse_schema_file(
            file.path(),
            &context.freeze(),
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
            assert!(table.primary_key_columns().is_empty());
        }
    }
}
