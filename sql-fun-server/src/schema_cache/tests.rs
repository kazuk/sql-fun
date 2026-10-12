use super::{SchemaCacheService, SchemaContextKey, SchemaSelection};
use ::sql_fun_sqlast::sem::{BaseParseContext, FullName, TableName};
use std::sync::Arc;

use super::test_helpers::SchemaProject;

#[tokio::test]
async fn shares_prefix_and_rebinds_datasource_metadata() {
    let project = SchemaProject::new();
    let service = project.service();
    let extension = project.schema("extension.sql", "CREATE TABLE extension_table (id int4);");
    let dump = project.schema("dump.sql", "CREATE TABLE app_table (id int4);");
    let hash = SchemaProject::hash(&dump);
    let products = project.selection("products", std::slice::from_ref(&extension), &dump, &hash);
    let orders = project.selection("orders", std::slice::from_ref(&extension), &dump, &hash);
    let first = service.initialize(&products).await.unwrap();
    let second = service.initialize(&orders).await.unwrap();
    assert_eq!(first.key(), second.key());
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
    assert_eq!(first.base_context().metadata().name(), "products");
    assert_eq!(second.base_context().metadata().name(), "orders");
    assert_eq!(
        first.base_context().metadata().connector().as_deref(),
        Some("products-connector")
    );
    assert_eq!(
        second.base_context().metadata().connector().as_deref(),
        Some("orders-connector")
    );
    assert_eq!(
        second.analysis().base_context().metadata().connector(),
        &None
    );
    drop(service);
    assert!(
        second
            .base_context()
            .get_table(&TableName::from(FullName::with_schema(
                "public",
                "app_table"
            )))
            .is_some()
    );
}

#[tokio::test]
async fn mismatch_populates_actual_hash_and_retry_reuses_result() {
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "CREATE TABLE before_change (id int4);");
    let expected = SchemaProject::hash(&dump);
    project.schema("dump.sql", "CREATE TABLE after_change (id int4);");
    let request = project.selection("products", &[], &dump, &expected);
    let error = service.initialize(&request).await.err().unwrap();
    assert!(error.to_string().contains("schema dump changed"));
    let actual = SchemaProject::hash(&dump);
    let root_key = SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()]);
    let key = root_key
        .layer(&actual, &dump.canonicalize().unwrap())
        .unwrap();
    let cached = service.layers.lookup(&key).unwrap();
    let retry = project.selection("products", &[], &dump, &actual);
    let loaded = service.initialize(&retry).await.unwrap();
    assert!(std::ptr::eq(
        loaded.analysis(),
        SchemaCacheService::successful(&cached).unwrap().as_ref()
    ));
    assert_eq!(loaded.actual_hash(), actual);
    assert!(
        service
            .initialize(&request)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("schema dump changed")
    );
}

#[tokio::test]
async fn candidate_hash_never_receives_actual_content_result() {
    let project = SchemaProject::new();
    let service = project.service();
    let root = SchemaCacheService::load_root(
        project.home(),
        &"17".parse().unwrap(),
        &["public".to_owned()],
        &SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()]),
    )
    .unwrap();
    let path = project
        .schema("dump.sql", "CREATE TABLE old_table (id int4);")
        .canonicalize()
        .unwrap();
    let old = SchemaProject::hash(&path);
    let token = service.hashes.begin(&path).unwrap();
    project.schema("dump.sql", "CREATE TABLE new_table (id int4);");
    service.hashes.invalidate(&path);
    let actual = SchemaProject::hash(&path);
    let loaded = service
        .resolve_layer(&root, &path, &old, token)
        .await
        .unwrap();
    assert_eq!(loaded.hash, actual);
    assert!(
        service
            .layers
            .lookup(&root.key.layer(&old, &path).unwrap())
            .is_none()
    );
    assert!(
        service
            .layers
            .lookup(&root.key.layer(&actual, &path).unwrap())
            .is_some()
    );
    assert_eq!(service.hashes.hash(&path).unwrap(), actual);
}

#[rstest::rstest]
#[case::syntax("CREATE TABLE broken (id")]
#[case::semantic("CREATE TABLE child (a int4 REFERENCES missing (a,b));")]
#[tokio::test]
async fn deterministic_failure_is_retained_and_mismatch_is_separate(#[case] sql: &str) {
    let project = SchemaProject::new();
    let service = project.service();
    let path = project.schema("bad.sql", sql).canonicalize().unwrap();
    let actual = SchemaProject::hash(&path);
    let request = project.selection("products", &[], &path, &"a".repeat(64));
    assert!(
        service
            .initialize(&request)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("schema dump changed")
    );
    let root_key = SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()]);
    let key = root_key.layer(&actual, &path).unwrap();
    let first = service.layers.lookup(&key).unwrap();
    assert!(first.is_err());
    let retry = project.selection("products", &[], &path, &actual);
    assert!(service.initialize(&retry).await.is_err());
    let second = service.layers.lookup(&key).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
}

#[tokio::test]
async fn io_failure_retries_and_diagnostics_keep_layer_order() {
    let project = SchemaProject::new();
    let service = project.service();
    let extension = project.schema("extension.sql", "DROP TABLE missing_extension;");
    let dump = project.schema("dump.sql", "DROP TABLE missing_app;");
    let actual = SchemaProject::hash(&dump);
    let request = project.selection("products", std::slice::from_ref(&extension), &dump, &actual);
    std::fs::remove_file(&dump).unwrap();
    assert!(service.initialize(&request).await.is_err());
    project.schema("dump.sql", "DROP TABLE missing_app;");
    let first = service.initialize(&request).await.unwrap();
    let second = service.initialize(&request).await.unwrap();
    let diagnostics = second.analysis().diagnostics();
    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[1].path(), extension.canonicalize().unwrap());
    assert!(!diagnostics[1].problems().is_empty());
    assert_eq!(diagnostics[2].path(), dump.canonicalize().unwrap());
    assert!(!diagnostics[2].problems().is_empty());
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
}

#[tokio::test]
async fn canonical_aliases_share_identity_and_order_changes_identity() {
    let project = SchemaProject::new();
    let service = project.service();
    let a = project.schema("a.sql", "CREATE TABLE a (id int4);");
    let b = project.schema("b.sql", "CREATE TABLE b (id int4);");
    let dump = project.schema("dump.sql", "-- application\n");
    let alias = project.home().join("alias.sql");
    std::os::unix::fs::symlink(&dump, &alias).unwrap();
    let hash = SchemaProject::hash(&dump);
    let forward = project.selection("products", &[a.clone(), b.clone()], &dump, &hash);
    let reversed = project.selection("products", &[b, a], &dump, &hash);
    let aliased = project.selection("products", &forward.extensions, &alias, &hash);
    let first = service.initialize(&forward).await.unwrap();
    let second = service.initialize(&aliased).await.unwrap();
    assert_eq!(first.key(), second.key());
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
    assert_ne!(
        first.key(),
        service.initialize(&reversed).await.unwrap().key()
    );
}

#[test]
fn fixed_identity_vectors_and_search_path_order() {
    let version = "17".parse().unwrap();
    let root = SchemaContextKey::root(&version, &["public".to_owned()]);
    assert_eq!(
        root.as_str(),
        "cd82693b72f7ae267a4d9221cfd31b3bd061271ae5b489c8a0cf3066e90fd418"
    );
    assert_ne!(
        SchemaContextKey::root(&version, &["a".to_owned(), "b".to_owned()]),
        SchemaContextKey::root(&version, &["b".to_owned(), "a".to_owned()])
    );
}

#[tokio::test]
async fn different_dumps_share_extension_prefix_but_keep_separate_final_layers() {
    let project = SchemaProject::new();
    let service = project.service();
    let extension = project.schema("shared.sql", "CREATE TABLE common (id int4);");
    let products_dump = project.schema("products.sql", "CREATE TABLE products (id int4);");
    let orders_dump = project.schema("orders.sql", "CREATE TABLE orders (id int4);");
    let products = project.selection(
        "products",
        std::slice::from_ref(&extension),
        &products_dump,
        &SchemaProject::hash(&products_dump),
    );
    let orders = project.selection(
        "orders",
        std::slice::from_ref(&extension),
        &orders_dump,
        &SchemaProject::hash(&orders_dump),
    );
    let first = service.initialize(&products).await.unwrap();
    let second = service.initialize(&orders).await.unwrap();
    assert_ne!(first.key(), second.key());
    assert!(Arc::ptr_eq(
        &first.analysis().diagnostics()[0],
        &second.analysis().diagnostics()[0]
    ));
    assert!(Arc::ptr_eq(
        &first.analysis().diagnostics()[1],
        &second.analysis().diagnostics()[1]
    ));
    let products_name = TableName::from(FullName::with_schema("public", "products"));
    assert!(first.base_context().get_table(&products_name).is_some());
    assert!(second.base_context().get_table(&products_name).is_none());
}

#[test]
fn server_owner_reuses_service_and_rejects_another_installation() {
    let first = SchemaProject::new();
    let second = SchemaProject::new();
    let api = crate::Api {
        schema_cache: std::sync::OnceLock::new(),
    };
    let service = api.schema_cache_service(first.home()).unwrap();
    assert!(std::ptr::eq(
        service,
        api.schema_cache_service(first.home()).unwrap()
    ));
    assert!(api.schema_cache_service(second.home()).is_err());
}

#[rstest::rstest]
#[case::missing("")]
#[case::unknown("unknown")]
fn selection_never_infers_a_datasource(#[case] datasource: &str) {
    let project = SchemaProject::new();
    let dump = project.schema("dump.sql", "-- application");
    let explicit = project.selection("products", &[], &dump, &SchemaProject::hash(&dump));
    assert!(
        SchemaSelection::new(
            datasource,
            &explicit.args,
            &explicit.metadata,
            &[],
            &dump,
            &explicit.expected_hash
        )
        .is_err()
    );
}

#[tokio::test]
async fn io_error_after_canonicalization_is_not_a_negative_entry() {
    let project = SchemaProject::new();
    let service = project.service();
    let root_key = SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()]);
    let root = SchemaCacheService::load_root(
        project.home(),
        &"17".parse().unwrap(),
        &["public".to_owned()],
        &root_key,
    )
    .unwrap();
    let path = project
        .schema("dump.sql", "-- application")
        .canonicalize()
        .unwrap();
    let hash = SchemaProject::hash(&path);
    let token = service.hashes.begin(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        service
            .resolve_layer(&root, &path, &hash, token)
            .await
            .is_err()
    );
    assert!(
        service
            .layers
            .lookup(&root.key.layer(&hash, &path).unwrap())
            .is_none()
    );
    project.schema("dump.sql", "-- application");
    let token = service.hashes.begin(&path).unwrap();
    assert!(
        service
            .resolve_layer(&root, &path, &hash, token)
            .await
            .unwrap()
            .outcome
            .is_ok()
    );
}

#[test]
fn initialization_rejects_omitted_search_path() {
    let args = ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())
        .unwrap()
        .build()
        .unwrap();
    let metadata = ::serde_json::from_value(
        ::serde_json::json!({"engine":"PostgreSQL", "dialect":"postgres", "version":"17"}),
    )
    .unwrap();
    let error = SchemaSelection::new(
        "default",
        &args,
        &metadata,
        &[],
        std::path::Path::new("explicit.sql"),
        &"a".repeat(64),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("search_path must be explicitly supplied")
    );
}
