//! Integration coverage for loading committed `PostgreSQL` builtin schemas.

use std::path::PathBuf;

use ::clap::Parser;
use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    sem::{BaseContext, BaseParseContext, FrozenBaseContext, FullName, TableName},
};

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
fn loads_builtin_and_user_schemas(#[case] version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let home = PathBuf::from(env!("SQL_FUN_HOME"));
    let metadata = manifest.join("sql_fun.metadata.toml");
    let args = SqlFunArgs::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.to_str().expect("metadata path is UTF-8"),
        "--sql-fun-home",
        home.to_str().expect("builtin path is UTF-8"),
        "--postgres-version",
        version,
    ])?;

    let collection = args.load_metadata()?;
    let selected = collection.get("default").expect("selected test metadata");
    let context = BaseContext::new(&args, selected)?;
    assert_master_reads(&context);
    let schema = home.join(format!("postgres/{version}/schema.sql"));
    let builtins = {
        let analysis_base = &FrozenBaseContext::from(context);
        PostgresDialect::parse_schema_file(&schema, analysis_base, &mut None, &args, selected)
    }?;
    assert_master_reads(builtins.base_context());
    let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
    assert!(
        builtins.get_table(&pg_class).is_some(),
        "PostgreSQL {version}"
    );
    let user_schema = tempfile::NamedTempFile::new()?;
    std::fs::write(
        user_schema.path(),
        "CREATE TABLE hoge (id int PRIMARY KEY); SELECT abs(id) + 1 AS next_id FROM hoge;",
    )?;
    let parsed = {
        let analysis_base = builtins.base_context();
        PostgresDialect::parse_schema_file(
            user_schema.path(),
            analysis_base,
            &mut None,
            &args,
            selected,
        )
    }?;
    assert_master_reads(parsed.base_context());
    let user_table = TableName::from(FullName::with_schema("public", "hoge"));
    assert!(
        parsed.get_table(&user_table).is_some(),
        "PostgreSQL {version}"
    );
    Ok(())
}

fn assert_master_reads(context: &impl BaseParseContext) {
    let name = FullName::with_schema("pg_catalog", "C");
    assert_eq!(
        context
            .lookup_collation(&FullName::from_local_name("C"))
            .resolved_name(),
        Some(&name)
    );
    let definition = context
        .conflict_masters()
        .qualified_collation(&name)
        .unwrap();
    assert_eq!(definition.encoding(), -1);
    assert_eq!(definition.provider(), "c");
    let database = context.conflict_masters().database().unwrap();
    let default = context
        .conflict_masters()
        .collation(database.default_collation())
        .unwrap();
    assert_eq!(default.provider(), "d");
    for method in ["btree", "hash"] {
        let name = FullName::with_schema("pg_catalog", "int4_ops");
        assert_eq!(
            context
                .lookup_operator_class(&FullName::from_local_name("int4_ops"), method)
                .resolved_name(),
            Some(&name)
        );
        let class = context
            .conflict_masters()
            .operator_class(&name, method)
            .unwrap();
        assert_eq!(class.family().access_method(), method);
        assert_eq!(
            class.input_type().qualified_name(),
            FullName::with_schema("pg_catalog", "int4")
        );
        assert!(class.default());
    }
}
