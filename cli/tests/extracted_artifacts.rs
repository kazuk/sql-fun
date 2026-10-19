//! Loader compatibility for the officially extracted `PostgreSQL` artifacts.
mod support;

use ::sql_fun_sqlast::sem::{ConflictInfoRead, FullName};
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
    // The real extracted declarations must also support opt-in loaded signatures.
    let frozen = sql_fun_sqlast::sem::FrozenBaseContext::from(context);
    let metadata = args.load_metadata().unwrap();
    let schema_context = sql_fun_sqlast::sem::SchemaFileContext::new(
        std::path::Path::new("extracted-signatures.sql"),
        &frozen,
        &args,
        metadata.get("default").unwrap(),
    );
    let types = sql_fun_sqlast::types::TypeContext::new(&schema_context);
    let scalar = types
        .function_by_exact_name(&FullName::with_schema("pg_catalog", "json_to_record"))
        .unwrap()
        .unwrap();
    assert!(!scalar.signatures()[0].result().returns_set());
    let series = types
        .function_by_exact_name(&FullName::with_schema("pg_catalog", "generate_series"))
        .unwrap()
        .unwrap();
    assert!(series.signatures().len() > 1);
    for signature in series.signatures() {
        assert!(signature.result().returns_set());
    }
    let keywords = types
        .function_by_exact_name(&FullName::with_schema("pg_catalog", "pg_get_keywords"))
        .unwrap()
        .unwrap();
    assert!(keywords.signatures()[0].result().returns_set());
    assert_ne!(keywords.signatures()[0].parameters(), []);
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
