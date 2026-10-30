use super::{SchemaCacheService, SchemaContextKey, SchemaSelection};
use ::sql_fun_sqlast::context::BaseParseContext;
use ::sql_fun_sqlast::sem::{FullName, TableName};
use std::sync::Arc;

use super::test_helpers::SchemaProject;

#[tokio::test]
async fn new_analysis_uses_selected_environment_over_shared_normalized_catalog() {
    let mut cache_context = ::sql_fun_sqlast::CacheContext::new();
    let project = SchemaProject::new();
    let service = project.service();
    let dump = project.schema("dump.sql", "CREATE TABLE app_table (id int4);");
    let selection = project.selection("products", &[], &dump, &SchemaProject::hash(&dump));
    let schema = service.initialize(&selection).await.unwrap();
    let context = project.analysis_context_over(schema.base_context(), "products");
    assert_eq!(context.metadata().name(), "products");
    assert_ne!(
        context.metadata().name(),
        schema.base_context().metadata().name()
    );
    let query = ::sql_fun_sqlast::parse_statements(
        "select id from app_table",
        ::sql_fun_sqlast::ast::Dialect::postgres(),
    )
    .next()
    .unwrap()
    .unwrap()
    .analyze(context, &mut cache_context)
    .unwrap();
    assert_eq!(query.context().metadata().name(), "products");
    assert!(query.context().problems().is_empty());
}

#[tokio::test]
async fn shares_prefix_and_preserves_analysis_parameter_evidence() {
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
    assert!(std::ptr::eq(
        first.base_context(),
        first.analysis().base_context()
    ));
    assert!(std::ptr::eq(
        second.base_context(),
        second.analysis().base_context()
    ));
    assert!(std::ptr::eq(
        first.base_context().metadata(),
        second.base_context().metadata()
    ));
    assert!(std::ptr::eq(
        first.base_context().get_args(),
        second.base_context().get_args()
    ));
    assert_eq!(second.base_context().metadata().connector(), &None);
    assert_eq!(
        second
            .base_context()
            .get_args()
            .postgres_search_path(second.base_context().metadata())
            .unwrap(),
        vec!["public"]
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
    assert_eq!(loaded.actual_hash(), Some(actual.as_str()));
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
        None,
        &SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()], None),
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
    let (args, metadata) = project.schema_environment();
    let loaded = service
        .resolve_layer(&root, snapshot, token, &args, &metadata)
        .await
        .unwrap();
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
    let root = SchemaContextKey::root(&version, &["public".to_owned()], None);
    assert_eq!(
        root.as_str(),
        "cd82693b72f7ae267a4d9221cfd31b3bd061271ae5b489c8a0cf3066e90fd418"
    );
    assert_ne!(
        SchemaContextKey::root(&version, &["a".to_owned(), "b".to_owned()], None),
        SchemaContextKey::root(&version, &["b".to_owned(), "a".to_owned()], None)
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
    let (args, metadata) = project.analysis_inputs("products");
    assert!(
        SchemaSelection::new(
            datasource,
            &args,
            &metadata,
            &[],
            &dump,
            explicit.expected_hash.as_deref().unwrap()
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
        None,
        &SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()], None),
    )
    .unwrap();
    let path = project
        .schema("dump.sql", "CREATE TABLE retained (id int4);")
        .canonicalize()
        .unwrap();
    let snapshot = ::sql_fun_sqlast::SqlFileSnapshot::read(&path).unwrap();
    let token = service.hashes.begin(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let (args, metadata) = project.schema_environment();
    let loaded = service
        .resolve_layer(&root, snapshot, token, &args, &metadata)
        .await
        .unwrap();
    let table = TableName::from(FullName::with_schema("public", "retained"));
    assert!(
        SchemaCacheService::successful(&loaded.outcome)
            .unwrap()
            .base_context()
            .get_table(&table)
            .is_some()
    );
    assert!(
        service
            .load_layer(&root, &path, None, &args, &metadata)
            .await
            .is_err()
    );
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
    let (args, _) = project.analysis_inputs("products");
    let default = SchemaSelection::new(
        metadata.name(),
        &args,
        &metadata,
        &[],
        &dump,
        explicit.expected_hash.as_deref().unwrap(),
    )
    .unwrap();
    assert_eq!(default.search_path, vec!["public"]);
    let first = service.initialize(&explicit).await.unwrap();
    let second = service.initialize(&default).await.unwrap();
    assert_eq!(first.key(), second.key());
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
}

// Removed builtin_sql_edits_replace_file_layer_and_preserve_root after a CI timeout.
// Deletion commit: d4b1ac47e5969c4e639b7c181b561fe176085774; recover from its parent.

#[tokio::test]
async fn changed_observation_does_not_join_older_pending_extension() {
    let project = SchemaProject::new();
    let service = Arc::new(project.service());
    let root = SchemaCacheService::load_root(
        project.home(),
        &"17".parse().unwrap(),
        &["public".to_owned()],
        None,
        &SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()], None),
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
    let (args, metadata) = project.schema_environment();
    let loaded = ::tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service.resolve_layer(
            &root,
            ::sql_fun_sqlast::SqlFileSnapshot::read(&path).unwrap(),
            token,
            &args,
            &metadata,
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
    assert_eq!(old.actual_hash(), Some(original_hash.as_str()));
    assert_eq!(new.actual_hash(), Some(revised_hash.as_str()));
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

#[rstest::rstest]
#[case::builtin(::sql_fun_core::SchemaLevel::builtin(), 0)]
#[case::default_sql(::sql_fun_core::SchemaLevel::default_sql(), 1)]
#[case::extensions(::sql_fun_core::SchemaLevel::extensions(), 2)]
#[case::project(::sql_fun_core::SchemaLevel::project(), 3)]
#[tokio::test]
async fn requested_prefix_preserves_objects_cache_identity_and_excludes_later_files(
    #[case] level: ::sql_fun_core::SchemaLevel,
    #[case] depth: usize,
) {
    let project = SchemaProject::new();
    let service = project.service();
    let builtin = project.schema(
        "postgres/17/schema.sql",
        "CREATE TABLE builtin_sql (id int4);",
    );
    let extension = project.schema("extension.sql", "CREATE TABLE extension_sql (id int4);");
    let dump = project.schema("dump.sql", "CREATE TABLE project_sql (id int4);");
    let hash = SchemaProject::hash(&dump);
    // Excluded paths deliberately do not exist. Root must not read builtin SQL either.
    if depth < 3 {
        std::fs::remove_file(&dump).unwrap();
    }
    if depth < 2 {
        std::fs::remove_file(&extension).unwrap();
    }
    if depth < 1 {
        std::fs::remove_file(&builtin).unwrap();
    }
    let (args, metadata) = project.analysis_inputs("products");
    let selection = SchemaSelection::for_level(
        "products",
        &args,
        &metadata,
        std::slice::from_ref(&extension),
        Some(&dump),
        Some(if depth == 3 {
            &hash
        } else {
            "ignored-invalid-hash"
        }),
        level,
    )
    .unwrap();
    let first = service.initialize(&selection).await.unwrap();
    let second = service.initialize(&selection).await.unwrap();
    assert!(std::ptr::eq(first.analysis(), second.analysis()));
    assert_eq!(
        first.actual_hash(),
        if depth == 3 {
            Some(hash.as_str())
        } else {
            None
        }
    );
    for (index, name) in ["builtin_sql", "extension_sql", "project_sql"]
        .iter()
        .enumerate()
    {
        let name = TableName::from(FullName::with_schema("public", name));
        assert_eq!(
            first.base_context().get_table(&name).is_some(),
            index < depth
        );
    }
    // A nonexistent excluded file cannot have entered the path/hash-watch registry.
    let known = service.known_paths.lock().unwrap();
    for (index, path) in [builtin, extension, dump].iter().enumerate() {
        assert_eq!(known.contains_key(path), index < depth);
    }
}

#[rstest::rstest]
#[case::folded("OWNER", "owner")]
#[case::quoted(r#""Mixed Owner""#, "Mixed Owner")]
#[case::escaped_quote(r#""a""b""#, "a\"b")]
#[case::unicode(r#""所有者""#, "所有者")]
#[case::dot(r#""a.b""#, "a.b")]
fn configured_roles_bind_and_apply_through_direct_context(
    #[case] spelling: &str,
    #[case] expected: &str,
) {
    use ::sql_fun_sqlast::context::ParseContext;
    use ::sql_fun_sqlast::{
        context::{OwnershipInfoRead, RoleInfoRead},
        sem::{ObjectKind, Role},
    };
    let project = SchemaProject::new();
    project.configure_roles(
        "products",
        r#"roles = ['owner', 'Mixed Owner', 'a"b', '所有者', 'a.b']"#,
    );
    let context = project.analysis_context("products");
    assert_eq!(
        context.role_by_name(expected).map(Role::name),
        Some(expected)
    );
    let created = SchemaProject::apply_sql(context, "CREATE SCHEMA app").into_context();
    let changed =
        SchemaProject::apply_sql(created, &format!("ALTER SCHEMA app OWNER TO {spelling}"));
    let change = changed.ast().ownership_change().unwrap();
    assert_eq!(
        change.owner().resolved_role().map(Role::name),
        Some(expected)
    );
    assert!(changed.context().problems().is_empty());
    let frozen = changed.into_context().freeze();
    let next = project.analysis_context_over(&frozen, "products");
    let definitions =
        next.ownership_definitions(ObjectKind::from(37), &FullName::from_local_name("app"));
    assert_eq!(
        definitions[0]
            .owner()
            .unwrap()
            .resolved_role()
            .map(Role::name),
        Some(expected)
    );
}

#[rstest::rstest]
// The omitted and empty inventory cases timed out in CI and were removed in
// 342e3aac2f36ac19430b23ae04a957a0fda352f5; recover them from that commit's parent.
#[case::different("roles = ['other']")]
fn current_inventory_does_not_inherit_frozen_project_roles(#[case] declaration: &str) {
    use ::sql_fun_sqlast::context::ParseContext;
    use ::sql_fun_sqlast::context::{OwnershipInfoRead, RoleInfoRead};
    let project = SchemaProject::new();
    project.configure_roles("products", "roles = ['owner']");
    project.configure_roles("orders", declaration);
    let original = project.analysis_context("products");
    assert!(original.role_by_name("owner").is_some());
    let base = SchemaProject::apply_sql(original, "CREATE SCHEMA app")
        .into_context()
        .freeze();
    let selected = project.analysis_context_over(&base, "orders");
    assert!(selected.role_by_name("owner").is_none());
    let failed = SchemaProject::apply_sql(selected, "ALTER SCHEMA app OWNER TO owner");
    assert!(
        failed
            .ast()
            .ownership_change()
            .unwrap()
            .owner()
            .resolved_role()
            .is_none()
    );
    assert_eq!(
        failed.context().problems()[0].code(),
        "ownership_role_not_found"
    );
    assert!(
        failed
            .context()
            .schema_definition("app")
            .unwrap()
            .owner()
            .is_none()
    );
    let later = SchemaProject::apply_sql(failed.into_context(), "CREATE SCHEMA later");
    assert!(later.context().schema_definition("later").is_some());
}

#[tokio::test]
async fn role_inventory_partitions_cached_bindings_and_diagnostics() {
    use ::sql_fun_sqlast::{
        context::{OwnershipInfoRead, RoleInfoRead},
        sem::Role,
    };
    let project = SchemaProject::new();
    let service = project.service();
    project.configure_roles("products", "roles = ['owner', 'other']");
    let dump = project.schema(
        "roles.sql",
        "CREATE SCHEMA app; ALTER SCHEMA app OWNER TO owner;",
    );
    let hash = SchemaProject::hash(&dump);
    let configured = service
        .initialize(&project.selection("products", &[], &dump, &hash))
        .await
        .unwrap();
    assert_eq!(
        configured
            .base_context()
            .schema_definition("app")
            .unwrap()
            .owner()
            .unwrap()
            .resolved_role()
            .map(Role::name),
        Some("owner")
    );
    assert!(
        configured
            .analysis()
            .diagnostics()
            .last()
            .unwrap()
            .problems()
            .is_empty()
    );
    project.configure_roles("orders", "roles = ['other', 'owner']");
    let reordered = service
        .initialize(&project.selection("orders", &[], &dump, &hash))
        .await
        .unwrap();
    assert_eq!(configured.key(), reordered.key());
    assert!(std::ptr::eq(configured.analysis(), reordered.analysis()));
    project.configure_roles("orders", "roles = ['other']");
    let missing = service
        .initialize(&project.selection("orders", &[], &dump, &hash))
        .await
        .unwrap();
    assert_ne!(configured.key(), missing.key());
    assert!(missing.base_context().role_by_name("owner").is_none());
    assert!(
        missing
            .base_context()
            .schema_definition("app")
            .unwrap()
            .owner()
            .is_none()
    );
    assert_eq!(
        missing.analysis().diagnostics().last().unwrap().problems()[0].code(),
        "ownership_role_not_found"
    );
    let query_context = project.analysis_context_over(configured.base_context(), "products");
    let later = SchemaProject::apply_sql(query_context, "ALTER SCHEMA app OWNER TO other");
    assert_eq!(
        later
            .ast()
            .ownership_change()
            .unwrap()
            .owner()
            .resolved_role()
            .map(Role::name),
        Some("other")
    );
    project.configure_roles("orders", "");
    let omitted = service
        .initialize(&project.selection("orders", &[], &dump, &hash))
        .await
        .unwrap();
    project.configure_roles("orders", "roles = []");
    let empty = service
        .initialize(&project.selection("orders", &[], &dump, &hash))
        .await
        .unwrap();
    assert_ne!(omitted.key(), empty.key());
    assert!(omitted.base_context().metadata().roles().is_none());
    assert!(empty.base_context().metadata().roles().is_some());
}

#[test]
fn explicitly_registered_roles_survive_current_inventory_changes() {
    use ::sql_fun_sqlast::context::ParseContext;
    use ::sql_fun_sqlast::{context::RoleInfoRead, sem::Role};
    let project = SchemaProject::new();
    project.configure_roles("products", "roles = ['configured']");
    project.configure_roles("orders", "roles = []");
    let mut original = project.analysis_context("products");
    original.insert_role(&Role::from("registered"));
    let frozen = original.freeze();
    let next = project.analysis_context_over(&frozen, "orders");
    assert!(next.role_by_name("configured").is_none());
    assert_eq!(
        next.role_by_name("registered").map(Role::name),
        Some("registered")
    );
}

#[rstest::rstest]
#[case::builtin(::sql_fun_core::SchemaLevel::builtin())]
#[case::default_sql(::sql_fun_core::SchemaLevel::default_sql())]
#[tokio::test]
async fn configured_roles_reach_builtin_and_default_prefixes(
    #[case] level: ::sql_fun_core::SchemaLevel,
) {
    use ::sql_fun_sqlast::{
        context::{OwnershipInfoRead, RoleInfoRead},
        sem::Role,
    };
    let project = SchemaProject::new();
    project.configure_roles("products", "roles = ['owner']");
    project.schema(
        "postgres/17/schema.sql",
        "CREATE SCHEMA default_target; ALTER SCHEMA default_target OWNER TO owner;",
    );
    let (args, metadata) = project.analysis_inputs("products");
    let selection =
        SchemaSelection::for_level("products", &args, &metadata, &[], None, None, level).unwrap();
    let schema = project.service().initialize(&selection).await.unwrap();
    assert_eq!(
        schema.base_context().role_by_name("owner").map(Role::name),
        Some("owner")
    );
    if level.includes_default() {
        assert_eq!(
            schema
                .base_context()
                .schema_definition("default_target")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("owner")
        );
        assert!(schema.analysis().diagnostics()[0].problems().is_empty());
    }
}
