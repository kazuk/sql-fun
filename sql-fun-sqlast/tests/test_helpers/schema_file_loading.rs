use ::clap::Parser;
use ::sql_fun_core::{SqlFunArgs, SqlFunMetadataCollection};
use ::sql_fun_sqlast::{
    ParseDumpFileError, ParsedSchemaDump, PostgresDialect,
    context::{BaseParseContext, FrozenBaseContext, GetSourcePathWithSpanOpt},
    names::{FullName, TableName},
    sem::{AnalysisProblem, problem::NameResolution},
};
use ::tempfile::NamedTempFile;
use std::path::Path;

#[cfg(feature = "sqlite")]
use ::sql_fun_sqlast::SqliteDialect;

pub(crate) type SchemaFileLoader =
    fn(&Path, &FrozenBaseContext) -> Result<ParsedSchemaDump, ParseDumpFileError>;

pub(crate) fn load_postgres(
    path: &Path,
    base: &FrozenBaseContext,
) -> Result<ParsedSchemaDump, ParseDumpFileError> {
    {
        let (args, selected) = environment("selected-postgres.toml").expect("fixture environment");
        let analysis_base = base;
        PostgresDialect::parse_schema_file(path, analysis_base, &mut None, &args, &selected)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) fn load_sqlite(
    path: &Path,
    base: &FrozenBaseContext,
) -> Result<ParsedSchemaDump, ParseDumpFileError> {
    {
        let (args, selected) = environment("selected-sqlite.toml").expect("fixture environment");
        let analysis_base = base;
        SqliteDialect::parse_schema_file(path, analysis_base, &mut None, &args, &selected)
    }
}

pub(crate) fn environment(
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

pub(crate) fn write_schema_file(sql: &str) -> Result<NamedTempFile, std::io::Error> {
    let file = NamedTempFile::new()?;
    std::fs::write(file.path(), sql)?;
    Ok(file)
}

pub(crate) fn assert_loaded_schema_and_provenance(
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

pub(crate) fn assert_recoverable_problem_is_per_file(
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

pub(crate) fn assert_fatal_file_leaves_base_reusable(
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

pub(crate) const fn is_postgres_syntax(error: &ParseDumpFileError) -> bool {
    matches!(error, ParseDumpFileError::Syn(_))
}

#[cfg(feature = "sqlite")]
pub(crate) const fn is_sqlite_syntax(error: &ParseDumpFileError) -> bool {
    matches!(error, ParseDumpFileError::SqliteSyn(_))
}
