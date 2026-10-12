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
    let key = project
        .builtin_key()
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
async fn snapshot_hash_and_analysis_stay_paired_after_file_changes() {
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
    let snapshot = ::sql_fun_sqlast::SqlFileSnapshot::read(&path).unwrap();
    let token = service.hashes.begin(&path).unwrap();
    project.schema("dump.sql", "CREATE TABLE new_table (id int4);");
    service.hashes.invalidate(&path);
    let actual = SchemaProject::hash(&path);
    let loaded = service.resolve_layer(&root, snapshot, token).await.unwrap();
    assert_eq!(loaded.hash.as_deref(), Some(old.as_str()));
    let table = TableName::from(FullName::with_schema("public", "old_table"));
    assert!(
        SchemaCacheService::successful(&loaded.outcome)
            .unwrap()
            .base_context()
            .get_table(&table)
            .is_some()
    );
    assert!(
        service
            .layers
            .lookup(&root.key.layer(&old, &path).unwrap())
            .is_some()
    );
    assert!(
        service
            .layers
            .lookup(&root.key.layer(&actual, &path).unwrap())
            .is_none()
    );
    assert!(service.hashes.lookup(&path, token).is_none());
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
    let key = project.builtin_key().layer(&actual, &path).unwrap();
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
async fn removed_file_does_not_change_an_already_read_snapshot() {
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
        .schema("dump.sql", "CREATE TABLE retained (id int4);")
        .canonicalize()
        .unwrap();
    let snapshot = ::sql_fun_sqlast::SqlFileSnapshot::read(&path).unwrap();
    let token = service.hashes.begin(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let loaded = service.resolve_layer(&root, snapshot, token).await.unwrap();
    let table = TableName::from(FullName::with_schema("public", "retained"));
    assert!(
        SchemaCacheService::successful(&loaded.outcome)
            .unwrap()
            .base_context()
            .get_table(&table)
            .is_some()
    );
    assert!(service.load_layer(&root, &path, None).await.is_err());
}

#[tokio::test]
async fn unspecified_search_path_shares_effective_default_identity() {
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "CREATE TABLE app (id int4);");
    let explicit = project.selection("products", &[], &dump, &SchemaProject::hash(&dump));
    let metadata: ::sql_fun_core::SqlFunMetadata = ::serde_json::from_value(
        ::serde_json::json!({"engine":"PostgreSQL", "dialect":"postgres", "version":"17"}),
    )
    .unwrap();
    let default = SchemaSelection::new(
        metadata.name(),
        &explicit.args,
        &metadata,
        &[],
        &dump,
        &explicit.expected_hash,
    )
    .unwrap();
    assert_eq!(default.search_path, vec!["public"]);
    let first = service.initialize(&explicit).await.unwrap();
    let second = service.initialize(&default).await.unwrap();
    assert_eq!(first.key(), second.key());
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
}

#[tokio::test]
async fn builtin_sql_edits_replace_file_layer_and_preserve_root() {
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "-- application");
    let request = project.selection("products", &[], &dump, &SchemaProject::hash(&dump));
    let first = service.initialize(&request).await.unwrap();
    let root_key = SchemaContextKey::root(&request.version, &request.search_path);
    let root = service.roots.lookup(&root_key).unwrap();
    assert!(root.hash.is_none());
    assert!(
        SchemaCacheService::successful(&root.outcome)
            .unwrap()
            .diagnostics()
            .is_empty()
    );
    let builtin = project.schema(
        "postgres/17/schema.sql",
        "CREATE TABLE new_builtin (id int4);",
    );
    service.hashes.invalidate(&builtin.canonicalize().unwrap());
    let second = service.initialize(&request).await.unwrap();
    assert_ne!(first.key(), second.key());
    let table = TableName::from(FullName::with_schema("public", "new_builtin"));
    assert!(first.base_context().get_table(&table).is_none());
    assert!(second.base_context().get_table(&table).is_some());
    assert!(Arc::ptr_eq(
        &root.outcome,
        &service.roots.lookup(&root_key).unwrap().outcome
    ));
    let repeated = service.initialize(&request).await.unwrap();
    assert!(std::ptr::eq(second.analysis(), repeated.analysis()));
}

#[tokio::test]
async fn changed_observation_does_not_join_older_pending_extension() {
    let project = SchemaProject::new();
    let service = Arc::new(project.service());
    let root = SchemaCacheService::load_root(
        project.home(),
        &"17".parse().unwrap(),
        &["public".to_owned()],
        &SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()]),
    )
    .unwrap();
    let path = project
        .schema("extension.sql", "CREATE TABLE old_extension (id int4);")
        .canonicalize()
        .unwrap();
    let old = SchemaProject::hash(&path);
    let token = service.hashes.begin(&path).unwrap();
    let pending = super::LayerLoadKey {
        parent: root.key.clone(),
        path: path.clone(),
        generation: crate::file_hash_cache::FileHashCache::generation(token),
        observed_hash: old,
    };
    let (started, ready) = ::tokio::sync::oneshot::channel();
    let (release, blocked) = ::tokio::sync::oneshot::channel();
    let owned = Arc::clone(&service);
    let old_root = root.clone();
    let older = ::tokio::spawn(async move {
        owned
            .inflight
            .load(&pending, move || async move {
                started.send(()).unwrap();
                blocked.await.unwrap();
                Ok(old_root)
            })
            .await
            .unwrap()
    });
    ready.await.unwrap();
    project.schema("extension.sql", "CREATE TABLE new_extension (id int4);");
    // Reuse the generation deliberately: absent notifications must not merge observations.
    let loaded = ::tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service.resolve_layer(
            &root,
            ::sql_fun_sqlast::SqlFileSnapshot::read(&path).unwrap(),
            token,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let table = TableName::from(FullName::with_schema("public", "new_extension"));
    assert!(
        SchemaCacheService::successful(&loaded.outcome)
            .unwrap()
            .base_context()
            .get_table(&table)
            .is_some()
    );
    release.send(()).unwrap();
    older.await.unwrap();
}

#[tokio::test]
async fn expected_hash_selects_historical_schema_after_dump_changes_or_deletion() {
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "CREATE TABLE original (id int4);");
    let alias = project.alias("selected.sql", &dump);
    let original_hash = SchemaProject::hash(&dump);
    let original = project.selection("products", &[], &alias, &original_hash);
    let first = service.initialize(&original).await.unwrap();
    project.schema("dump.sql", "CREATE TABLE revised (id int4);");
    service.hashes.invalidate(&dump.canonicalize().unwrap());
    let revised_hash = SchemaProject::hash(&dump);
    let revised = project.selection("products", &[], &alias, &revised_hash);
    let second = service.initialize(&revised).await.unwrap();
    assert_ne!(first.key(), second.key());
    std::fs::remove_file(&dump).unwrap();
    let old = service.initialize(&original).await.unwrap();
    let new = service.initialize(&revised).await.unwrap();
    assert!(std::ptr::eq(first.analysis(), old.analysis()));
    assert!(std::ptr::eq(second.analysis(), new.analysis()));
    assert_eq!(old.actual_hash(), original_hash);
    assert_eq!(new.actual_hash(), revised_hash);
    let table = TableName::from(FullName::with_schema("public", "original"));
    assert!(old.base_context().get_table(&table).is_some());
    assert!(new.base_context().get_table(&table).is_none());
}

#[tokio::test]
async fn expected_hash_retains_deterministic_error_after_dump_repair() {
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "CREATE TABLE broken (id");
    let failed_hash = SchemaProject::hash(&dump);
    let failed = project.selection("products", &[], &dump, &failed_hash);
    let original_error = service.initialize(&failed).await.err().unwrap().to_string();
    project.schema("dump.sql", "CREATE TABLE repaired (id int4);");
    std::fs::remove_file(&dump).unwrap();
    assert_eq!(
        service.initialize(&failed).await.err().unwrap().to_string(),
        original_error
    );
}
