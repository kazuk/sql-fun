//! Integration coverage for loading committed `PostgreSQL` builtin schemas.

use std::path::PathBuf;

use ::clap::Parser;
use ::sql_fun_sqlast::context::BaseParseContext;
use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    sem::{BaseContext, FrozenBaseContext, FullName, TableName},
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
    // The shipped catalog's pg_seclabels joins use implicit tableoid attributes.
    for problem in builtins.problems() {
        assert_ne!(
            problem.code(),
            "column_not_found_in_table",
            "PostgreSQL {version}: {problem:?}"
        );
    }
    if version == "18" {
        // #572/#727: retain complete catalog initialization and the affected
        // pg_policies view's real array output type.
        assert!(builtins.problems().is_empty(), "{:?}", builtins.problems());
        let sql = "SELECT roles FROM pg_catalog.pg_policies";
        let context = ::sql_fun_sqlast::sem::SchemaFileContext::new(
            std::path::Path::new("array-query.sql"),
            builtins.base_context(),
            &args,
            selected,
        );
        let analyzed =
            ::sql_fun_sqlast::parse_statements(sql, ::sql_fun_sqlast::ast::Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context)?;
        assert!(
            analyzed.context().problems().is_empty(),
            "{sql}: {:?}",
            analyzed.context().problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(::sql_fun_sqlast::sem::TypeReference::from_master_type_name("name").array_type()),
            "{sql}"
        );
    }
    let sql = "SELECT c.tableoid FROM pg_catalog.pg_class c";
    let mut query_context = ::sql_fun_sqlast::sem::SchemaFileContext::new(
        std::path::Path::new("system-column.sql"),
        builtins.base_context(),
        &args,
        selected,
    );
    ::sql_fun_sqlast::sem::MutSourcePathWithSpan::set_source_path_with_span(
        &mut query_context,
        &::sql_fun_sqlast::sem::SourcePathWithSpan::new(
            "system-column.sql",
            &::sql_fun_sqlast::StringSpan::new(0, sql.len()),
        ),
    );
    let analyzed =
        ::sql_fun_sqlast::parse_statements(sql, ::sql_fun_sqlast::ast::Dialect::postgres())
            .next()
            .unwrap()?
            .analyze(query_context)?;
    assert!(analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let result = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        result.col_type(),
        &Some(::sql_fun_sqlast::sem::TypeReference::from_master_type_name(
            "oid"
        ))
    );
    let binding = result
        .expression()
        .unwrap()
        .system_column_reference()
        .unwrap();
    assert_eq!(
        binding.relation(),
        &FullName::with_schema("pg_catalog", "pg_class")
    );
    assert_eq!(binding.attribute().as_str(), "tableoid");
    let table = builtins.get_table(&pg_class).unwrap();
    assert!(
        table
            .get_column_def(&::sql_fun_sqlast::sem::ColumnName::from("tableoid"))
            .is_none()
    );
    let user_schema = tempfile::NamedTempFile::new()?;
    std::fs::write(
        user_schema.path(),
        "CREATE TABLE hoge (id int PRIMARY KEY);
         CREATE TYPE mood AS ENUM ('ok');
         CREATE TYPE pair AS (x integer);
         CREATE DOMAIN number AS integer;
         CREATE DOMAIN nested_number AS number;
         CREATE DOMAIN numbers AS integer[];
         SELECT abs(id) + 1 AS next_id FROM hoge;",
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
    // This existing bootstrap test intentionally uses the real catalog/schema loader.
    for (name, code) in [
        ("hoge", 'C'),
        ("mood", 'E'),
        ("pair", 'C'),
        ("number", 'N'),
        ("nested_number", 'N'),
        ("numbers", 'A'),
    ] {
        let definition = parsed
            .get_type(&FullName::from_local_name(name))
            .expect("DDL type");
        assert_eq!(
            definition
                .category(&parsed)
                .map(sql_fun_sqlast::sem::TypeCategory::code),
            Some(code)
        );
    }

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
    let definition = context.collation_definition(&name).unwrap();
    assert_eq!(definition.encoding(), -1);
    assert_eq!(definition.provider(), "c");
    let database = context.database_collation().unwrap();
    let default = context
        .collation_definition(&database.default_collation().qualified_name())
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
        let class = context.operator_class_definition(&name, method).unwrap();
        assert_eq!(class.family().access_method(), method);
        assert_eq!(
            class.input_type().qualified_name(),
            FullName::with_schema("pg_catalog", "int4")
        );
        assert!(class.default());
    }
}
