//! Loader compatibility for the officially extracted PostgreSQL artifacts.
mod support;

use std::path::PathBuf;

#[test]
fn extracted_catalogs_load_and_schema_is_postgresql_syntax() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    for version in ["17", "18"] {
        let catalog = root.join("sql-fun-home/postgres").join(version);
        let fixture = support::CatalogProject::new();
        let args = fixture.arguments(&catalog);
        sql_fun_sqlast::sem::BaseContext::new(args).unwrap();
        let schema = std::fs::read_to_string(catalog.join("schema.sql")).unwrap();
        pg_query::parse(&schema).unwrap();
        let mut extensions: sql_fun_core::PostgresExtensionsCollection =
            "uuid-ossp@1.1".parse().unwrap();
        extensions
            .resolve_versions(catalog.join("extension"))
            .unwrap();
    }
}
