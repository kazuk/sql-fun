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
    let AstAndContextPair(_, context, _) = SelectStatement::analyze_select(
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
    let AstAndContextPair(ast, context, _) = crate::sem::SemAst::semantic_analysis_sqlite_stmt(
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

mod test_helpers {
    use std::path::Path;

    use crate::sem::{
        BaseContext, BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext,
        SourcePathWithSpan,
    };
    use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata, SqlFunMetadataCollection};

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
        let mut context =
            SchemaFileContext::new(Path::new("query.sql"), &FrozenBaseContext::from(base));
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
