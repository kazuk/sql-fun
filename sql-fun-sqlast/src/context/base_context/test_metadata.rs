//! The caller's selected metadata survives context initialization and analysis.

use crate::{
    AstAndContextPair, PostgresDialect,
    sem::SelectStatement,
    syn::{ListOpt, Opt},
};

#[test]
fn retains_selected_postgres_metadata_without_reloading_default()
-> Result<(), Box<dyn std::error::Error>> {
    let metadata = test_helpers::selected_metadata("selected-postgres.toml")?;
    let context = test_helpers::context(&metadata)?;
    let syntax = PostgresDialect::parse("select 42 as n")?;
    let select = syntax
        .get_stmts()
        .get(0)
        .get_stmt()
        .as_select_stmt()
        .as_inner()
        .expect("SELECT");
    let AstAndContextPair(_, context) = SelectStatement::analyze_select(
        context,
        select,
        &PostgresDialect::scan("select 42 as n")?,
    )?;
    test_helpers::assert_selected_metadata(&context, &metadata);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn retains_selected_sqlite_metadata_through_group_resolution()
-> Result<(), Box<dyn std::error::Error>> {
    let metadata = test_helpers::selected_metadata("selected-sqlite.toml")?;
    let context = test_helpers::context(&metadata)?;
    let statement = crate::SqliteDialect::parse("select q.n as n from ((select 42 as n)) as q")?
        .expect("statement")
        .into_plain_statement()
        .expect("plain SELECT");
    test_helpers::assert_selected_metadata(&context, &metadata);
    let AstAndContextPair(ast, context) = crate::sem::SemAst::semantic_analysis_sqlite_stmt(
        context,
        statement,
        &::sql_fun_core::IVec::default(),
    )?;
    let query = ast.into_select_statemtnt().expect("SELECT");
    assert_eq!(
        query
            .get_result_column_def(&crate::sem::ColumnName::from("n"))
            .expect("n")
            .get_type_cloned(),
        Some(crate::sem::PgBuiltInType::int4())
    );
    test_helpers::assert_selected_metadata(&context, &metadata);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn selected_environment_does_not_inherit_or_reconcile_frozen_configuration()
-> testresult::TestResult {
    use crate::context::{BaseParseContext, ParseContext};
    let mut cache_context = crate::CacheContext::new();
    let base = crate::test_helpers::TestParseContext::default().freeze();
    let selected = test_helpers::selected_metadata("selected-sqlite.toml")?;
    let context = test_helpers::context_over_catalog(&base, &selected)?;
    assert_eq!(
        base.metadata().sql_dialect(),
        ::sql_fun_core::SqlDialect::PostgreSQL
    );
    assert_eq!(context.dialect(), crate::ast::Dialect::sqlite());
    // Args remain available as supplied, but their dialect override does not
    // replace the selected metadata's analysis dialect.
    assert_eq!(
        context.get_args().sql_dialect(context.metadata())?,
        ::sql_fun_core::SqlDialect::PostgreSQL
    );
    assert_eq!(context.metadata().name(), selected.name());
    let analyzed = crate::parse_statements(
        "select cast(?1 as integer) as n",
        crate::ast::Dialect::sqlite(),
    )
    .next()
    .unwrap()?
    .analyze(context, &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
    assert_eq!(analyzed.context().metadata().name(), selected.name());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn selected_dialect_does_not_depend_on_the_engine_name() -> testresult::TestResult {
    use crate::context::{BaseParseContext, ParseContext};
    let metadata = test_helpers::sqlite_compatible_engine()?;
    let context = crate::test_helpers::TestParseContext::default().with_metadata(&metadata);
    assert_eq!(context.dialect(), crate::ast::Dialect::sqlite());
    assert_eq!(
        context.metadata().engine().as_custom().map(String::as_str),
        Some("custom-sqlite-engine")
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn statement_rejects_a_different_metadata_dialect() -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let context = crate::test_helpers::TestParseContext::default();
    let statement = crate::parse_statements("select 1", crate::ast::Dialect::sqlite())
        .next()
        .unwrap()?;
    let error = statement
        .analyze(context, &mut cache_context)
        .err()
        .expect("dialect mismatch");
    assert!(error.to_string().contains("Sqlite"));
    assert!(error.to_string().contains("Postgres"));
    Ok(())
}

mod test_helpers {
    use crate::{
        context::{
            BaseContext, BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan,
            SchemaFileContext,
        },
        sem::SourcePathWithSpan,
    };
    use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata, SqlFunMetadataCollection};
    use std::path::Path;

    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite_compatible_engine() -> Result<SqlFunMetadata, ::serde_json::Error> {
        ::serde_json::from_value(::serde_json::json!({
            "engine": "custom-sqlite-engine", "dialect": "SQLite", "version": "3"
        }))
    }

    #[cfg(feature = "sqlite")]
    pub(super) fn context_over_catalog(
        base: &FrozenBaseContext,
        metadata: &SqlFunMetadata,
    ) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
        let args = SqlFunArgs::builder(&std::collections::HashMap::new())?
            .sql_dialect(::sql_fun_core::SqlDialect::PostgreSQL)
            .build()?;
        Ok(SchemaFileContext::new(
            Path::new("query.sql"),
            base,
            &args,
            metadata,
        ))
    }

    pub(super) fn selected_metadata(
        fixture: &str,
    ) -> Result<SqlFunMetadata, Box<dyn std::error::Error>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture);
        let mut collection = SqlFunMetadataCollection::load_from(path)?;
        Ok(collection.remove("analytics").expect("selected metadata"))
    }

    pub(super) fn context(
        metadata: &SqlFunMetadata,
    ) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
        let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
            "sqlfun",
            "--metadata-file",
            "this-metadata-file-does-not-exist.toml",
            "--sql-fun-home",
            env!("SQL_FUN_HOME"),
            "--postgres-version",
            "17",
        ])?;
        let base = BaseContext::new(&args, metadata)?;
        assert_selected_metadata(&base, metadata);
        let mut context = {
            let analysis_base = &FrozenBaseContext::from(base);
            SchemaFileContext::new(Path::new("query.sql"), analysis_base, &args, metadata)
        };
        context.set_source_path_with_span(&SourcePathWithSpan::new(
            "query.sql",
            &crate::StringSpan::default(),
        ));
        Ok(context)
    }

    pub(super) fn assert_selected_metadata<C: BaseParseContext>(
        context: &C,
        selected: &SqlFunMetadata,
    ) {
        let metadata = context.metadata();
        assert_eq!(metadata.name(), "analytics");
        assert_eq!(metadata.sql_dialect(), selected.sql_dialect());
        assert_eq!(metadata.engine_version(), selected.engine_version());
        assert_eq!(metadata.search_path(), selected.search_path());
        assert_eq!(metadata.connector(), selected.connector());
        assert_eq!(
            context.get_search_path()[0].as_str(),
            selected.search_path().as_ref().expect("search path")[0]
        );
    }
}
