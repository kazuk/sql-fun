//! Loader compatibility for the officially extracted `PostgreSQL` artifacts.
mod support;

use ::sql_fun_sqlast::sem::MasterInfoRead;
use std::path::PathBuf;

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
fn extracted_catalogs_load_and_schema_is_postgresql_syntax(#[case] version: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let catalog = root.join("sql-fun-home/postgres").join(version);
    let fixture = support::CatalogProject::new();
    let args = fixture.arguments(&catalog);
    let context = sql_fun_sqlast::sem::BaseContext::new(
        &args,
        args.load_metadata()
            .expect("fixture metadata")
            .get("default")
            .expect("selected test metadata"),
    )
    .unwrap();
    assert!(context.conflict_masters().database().is_some());
    assert_ne!(context.conflict_masters().collations(), []);
    assert_ne!(context.conflict_masters().operator_classes(), []);
    let schema = std::fs::read_to_string(catalog.join("schema.sql")).unwrap();
    pg_query::parse(&schema).unwrap();
    let mut extensions: sql_fun_core::PostgresExtensionsCollection =
        "uuid-ossp@1.1".parse().unwrap();
    extensions
        .resolve_versions(catalog.join("extension"))
        .unwrap();
}

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
fn master_payloads_preserve_identities_relationships_and_defaults(#[case] version: &str) {
    let masters = support::MasterDefinitions::builtin(version);
    masters.assert_contract();
}
