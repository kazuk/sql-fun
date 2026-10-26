//! Direct loader coverage for immutable bases and per-file results.

use ::sql_fun_sqlast::ParseDumpFileError;

use self::test_helpers::{
    SchemaFileLoader, assert_fatal_file_leaves_base_reusable, assert_loaded_schema_and_provenance,
    assert_recoverable_problem_is_per_file, is_postgres_syntax, load_postgres,
    selected_frozen_base, write_schema_file,
};
#[cfg(feature = "sqlite")]
use self::test_helpers::{is_sqlite_syntax, load_sqlite};

mod test_helpers {
    use std::path::Path;

    use ::clap::Parser;
    use ::sql_fun_core::{SqlFunArgs, SqlFunMetadataCollection};
    use ::sql_fun_sqlast::context::BaseParseContext;
    use ::sql_fun_sqlast::{
        ParseDumpFileError, ParsedSchemaDump, PostgresDialect,
        sem::{
            AnalysisProblem, BaseContext, FrozenBaseContext, FullName, GetSourcePathWithSpanOpt,
            TableName, problem::NameResolution,
        },
    };
    use ::tempfile::NamedTempFile;

    #[cfg(feature = "sqlite")]
    use ::sql_fun_sqlast::SqliteDialect;

    pub(super) type SchemaFileLoader =
        fn(&Path, &FrozenBaseContext) -> Result<ParsedSchemaDump, ParseDumpFileError>;

    pub(super) fn load_postgres(
        path: &Path,
        base: &FrozenBaseContext,
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        {
            let (args, selected) =
                environment("selected-postgres.toml").expect("fixture environment");
            let analysis_base = base;
            PostgresDialect::parse_schema_file(path, analysis_base, &mut None, &args, &selected)
        }
    }

    #[cfg(feature = "sqlite")]
    pub(super) fn load_sqlite(
        path: &Path,
        base: &FrozenBaseContext,
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        {
            let (args, selected) =
                environment("selected-sqlite.toml").expect("fixture environment");
            let analysis_base = base;
            SqliteDialect::parse_schema_file(path, analysis_base, &mut None, &args, &selected)
        }
    }

    fn environment(
        fixture: &str,
    ) -> Result<(SqlFunArgs, ::sql_fun_core::SqlFunMetadata), Box<dyn std::error::Error>> {
        let metadata_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture);
        let mut metadata = SqlFunMetadataCollection::load_from(metadata_path)?;
        let selected = metadata
            .remove("analytics")
            .expect("explicitly selected fixture metadata");
        let args = SqlFunArgs::try_parse_from([
            "sqlfun",
            "--metadata-file",
            "this-metadata-file-does-not-exist.toml",
            "--sql-fun-home",
            env!("SQL_FUN_HOME"),
            "--postgres-version",
            "17",
        ])?;
        Ok((args, selected))
    }

    pub(super) fn selected_frozen_base(
        fixture: &str,
    ) -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
        let (args, selected) = environment(fixture)?;
        Ok(BaseContext::new(&args, &selected)?.into())
    }

    pub(super) fn write_schema_file(sql: &str) -> Result<NamedTempFile, std::io::Error> {
        let file = NamedTempFile::new()?;
        std::fs::write(file.path(), sql)?;
        Ok(file)
    }

    pub(super) fn assert_loaded_schema_and_provenance(
        loader: SchemaFileLoader,
        base: &FrozenBaseContext,
        file: &NamedTempFile,
        sql: &str,
        schema: &str,
        table: &str,
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        let table_name = TableName::from(FullName::with_schema(schema, table));
        assert!(base.get_table(&table_name).is_none());

        let parsed = loader(file.path(), base)?;

        assert!(parsed.get_table(&table_name).is_some());
        assert!(base.get_table(&table_name).is_none());
        let provenance = parsed
            .get_source_path_with_span()
            .expect("loaded statement provenance");
        assert_eq!(provenance.source_path(), file.path());
        assert_eq!(provenance.span().get_from(sql), Some(sql));
        Ok(parsed)
    }

    fn has_missing_table_problem(problems: &[AnalysisProblem], expected: &TableName) -> bool {
        problems.iter().any(|problem| match problem {
            AnalysisProblem::NameResolution(NameResolution::TableNotFound(actual)) => {
                actual.as_str() == expected.to_string()
            }
            _ => false,
        })
    }

    pub(super) fn assert_recoverable_problem_is_per_file(
        loader: SchemaFileLoader,
        base: &FrozenBaseContext,
        schema: &str,
        problem_file: &NamedTempFile,
        clean_file: &NamedTempFile,
    ) -> Result<(), ParseDumpFileError> {
        let missing_table = TableName::from(FullName::from_local_name("missing_from_layer"));
        let created_table = TableName::from(FullName::with_schema(schema, "recovered_table"));
        let parsed = loader(problem_file.path(), base)?;

        assert!(!parsed.problems().is_empty());
        assert!(has_missing_table_problem(parsed.problems(), &missing_table));
        assert!(parsed.get_table(&created_table).is_some());
        assert_eq!(
            parsed
                .get_source_path_with_span()
                .expect("recoverable statement provenance")
                .source_path(),
            problem_file.path()
        );
        let next_file = loader(clean_file.path(), parsed.base_context())?;
        assert!(next_file.problems().is_empty());
        assert!(has_missing_table_problem(parsed.problems(), &missing_table));
        Ok(())
    }

    pub(super) fn assert_fatal_file_leaves_base_reusable(
        loader: SchemaFileLoader,
        base: &FrozenBaseContext,
        schema: &str,
        fatal_file: &NamedTempFile,
        followup_file: &NamedTempFile,
    ) -> Result<ParseDumpFileError, Box<dyn std::error::Error>> {
        let partial_table = TableName::from(FullName::with_schema(schema, "unpublished_layer"));
        let followup_table = TableName::from(FullName::with_schema(schema, "usable_base"));
        let failure = loader(fatal_file.path(), base).expect_err("fatal file syntax");

        assert!(base.get_table(&partial_table).is_none());
        assert!(
            base.get_type(&FullName::with_schema("pg_catalog", "int4"))
                .is_some()
        );
        let followup = loader(followup_file.path(), base)?;
        assert!(followup.get_table(&followup_table).is_some());
        assert!(base.get_table(&followup_table).is_none());
        Ok(failure)
    }

    pub(super) const fn is_postgres_syntax(error: &ParseDumpFileError) -> bool {
        matches!(error, ParseDumpFileError::Syn(_))
    }

    #[cfg(feature = "sqlite")]
    pub(super) const fn is_sqlite_syntax(error: &ParseDumpFileError) -> bool {
        matches!(error, ParseDumpFileError::SqliteSyn(_))
    }
}

#[rstest::rstest]
#[case::postgres(
    load_postgres as SchemaFileLoader,
    "selected-postgres.toml",
    "analytics",
    "direct_postgres_load",
    "CREATE TABLE direct_postgres_load (id INTEGER)"
)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite(
        load_sqlite as SchemaFileLoader,
        "selected-sqlite.toml",
        "public",
        "direct_sqlite_load",
        "CREATE TABLE direct_sqlite_load (id)"
    )
)]
fn direct_loader_keeps_catalog_and_file_provenance(
    #[case] loader: SchemaFileLoader,
    #[case] fixture: &str,
    #[case] schema: &str,
    #[case] table: &str,
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = selected_frozen_base(fixture)?;
    let file = write_schema_file(sql)?;

    assert_loaded_schema_and_provenance(loader, &base, &file, sql, schema, table)?;
    Ok(())
}

#[rstest::rstest]
#[case::postgres(
    load_postgres as SchemaFileLoader,
    "selected-postgres.toml",
    "analytics",
    "DROP TABLE missing_from_layer; CREATE TABLE recovered_table (id INTEGER);",
    "CREATE TABLE clean_postgres_file (id INTEGER);"
)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite(
        load_sqlite as SchemaFileLoader,
        "selected-sqlite.toml",
        "public",
        "DROP TABLE missing_from_layer; CREATE TABLE recovered_table (id);",
        "CREATE TABLE clean_sqlite_file (id);"
    )
)]
fn loader_returns_recoverable_problems_on_only_their_file(
    #[case] loader: SchemaFileLoader,
    #[case] fixture: &str,
    #[case] schema: &str,
    #[case] problem_sql: &str,
    #[case] clean_sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = selected_frozen_base(fixture)?;
    let problem_file = write_schema_file(problem_sql)?;
    let clean_file = write_schema_file(clean_sql)?;

    assert_recoverable_problem_is_per_file(loader, &base, schema, &problem_file, &clean_file)?;
    Ok(())
}

#[rstest::rstest]
#[case::postgres(
    load_postgres as SchemaFileLoader,
    is_postgres_syntax as fn(&ParseDumpFileError) -> bool,
    "selected-postgres.toml",
    "analytics",
    "CREATE TABLE unpublished_layer (id INTEGER); CREATE TABLE broken_postgres (id",
    "CREATE TABLE usable_base (id INTEGER)"
)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite(
        load_sqlite as SchemaFileLoader,
        is_sqlite_syntax as fn(&ParseDumpFileError) -> bool,
        "selected-sqlite.toml",
        "public",
        "CREATE TABLE unpublished_layer (id); CREATE TABLE broken_sqlite (id",
        "CREATE TABLE usable_base (id)"
    )
)]
fn fatal_syntax_does_not_publish_partial_file_or_invalidate_base(
    #[case] loader: SchemaFileLoader,
    #[case] expected_syntax: fn(&ParseDumpFileError) -> bool,
    #[case] fixture: &str,
    #[case] schema: &str,
    #[case] fatal_sql: &str,
    #[case] followup_sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = selected_frozen_base(fixture)?;
    let fatal_file = write_schema_file(fatal_sql)?;
    let followup_file = write_schema_file(followup_sql)?;

    let failure =
        assert_fatal_file_leaves_base_reusable(loader, &base, schema, &fatal_file, &followup_file)?;
    assert!(expected_syntax(&failure));
    Ok(())
}
