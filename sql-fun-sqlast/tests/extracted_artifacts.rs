//! Loader compatibility for the officially extracted `PostgreSQL` artifacts.
mod extracted_artifacts_helpers {
    pub(crate) mod heavy;
}

use self::extracted_artifacts_helpers::heavy;

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
    let fixture = heavy::CatalogProject::new();
    let args = fixture.arguments(&catalog);
    let context = ::sql_fun_sqlast::sem::BaseContext::new(
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
    ::pg_query::parse(&schema).unwrap();
    let mut extensions: ::sql_fun_core::PostgresExtensionsCollection =
        "uuid-ossp@1.1".parse().unwrap();
    extensions
        .resolve_versions(catalog.join("extension"))
        .unwrap();
}

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
#[ignore = "Blocked by #749: builtin scalar return types are loaded as empty composites"]
fn installed_generation_bootstraps_builtin_and_user_schemas(#[case] version: &str) {
    let fixture = heavy::InitializedCatalog::new(version);
    fixture.assert_semantics(version);
}
