//! Direct loader coverage for immutable bases and per-file results.

// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use ::sql_fun_sqlast::ParseDumpFileError;

use crate::test_helpers::schema_file_loading::{
    SchemaFileLoader, assert_fatal_file_leaves_base_reusable, assert_loaded_schema_and_provenance,
    assert_recoverable_problem_is_per_file, is_postgres_syntax, load_postgres, write_schema_file,
};
#[cfg(feature = "sqlite")]
use crate::test_helpers::schema_file_loading::{is_sqlite_syntax, load_sqlite};

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
    let base = test_helpers::heavy::schema_file_loading::selected_frozen_base(fixture)?;
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
    let base = test_helpers::heavy::schema_file_loading::selected_frozen_base(fixture)?;
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
    let base = test_helpers::heavy::schema_file_loading::selected_frozen_base(fixture)?;
    let fatal_file = write_schema_file(fatal_sql)?;
    let followup_file = write_schema_file(followup_sql)?;

    let failure =
        assert_fatal_file_leaves_base_reusable(loader, &base, schema, &fatal_file, &followup_file)?;
    assert!(expected_syntax(&failure));
    Ok(())
}
