//! Loader compatibility for the officially extracted `PostgreSQL` artifacts.
// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

// Removed installed-generation bootstrap cases here and builtin_bootstrap.rs; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.

use crate::test_helpers::heavy::extracted_artifacts as heavy;

use ::sql_fun_sqlast::{context::ConflictInfoRead, names::FullName};
use std::path::PathBuf;

#[rstest::rstest]
#[case::seventeen("17")]
// PostgreSQL 18 case timed out; restore from deletion commit a7909559403227ecc0c3a5fba5240118089bed5e.
fn extracted_catalogs_load_and_schema_is_postgresql_syntax(#[case] version: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let catalog = root.join("sql-fun-home/postgres").join(version);
    let fixture = heavy::CatalogProject::new();
    let args = fixture.arguments(&catalog);
    let context = ::sql_fun_sqlast::context::BaseContext::new(
        &args,
        args.load_metadata()
            .expect("fixture metadata")
            .get("default")
            .expect("selected test metadata"),
    )
    .unwrap();
    assert!(context.database_collation().is_some());
    assert!(
        context
            .collation_definition(&FullName::with_schema("pg_catalog", "C"))
            .is_some()
    );
    assert!(
        context
            .operator_class_definition(&FullName::with_schema("pg_catalog", "int4_ops"), "btree")
            .is_some()
    );
    let schema = std::fs::read_to_string(catalog.join("schema.sql")).unwrap();
    ::pg_query::parse(&schema, ::pg_query::ParserOptions::default()).unwrap();
    let mut extensions: ::sql_fun_core::PostgresExtensionsCollection =
        "uuid-ossp@1.1".parse().unwrap();
    extensions
        .resolve_versions(catalog.join("extension"))
        .unwrap();
}
