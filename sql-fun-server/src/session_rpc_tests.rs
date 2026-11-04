use crate::{Api, schema_cache::test_helpers::SchemaProject, test_helpers::heavy};
use std::sync::{Arc, OnceLock};

fn api() -> Arc<Api> {
    Arc::new(Api {
        schema_cache: OnceLock::new(),
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn initialization_reuses_cache_across_connections_with_local_handles() {
    // Real authenticated sockets verify session dispatch; catalog data is synthetic.
    let project = SchemaProject::new();
    let api = api();
    let request = project.rpc_request("DROP TABLE missing;");
    let mut first = heavy::Connection::start_with_api(4096, 2, &api).await;
    let mut second = heavy::Connection::start_with_api(4096, 2, &api).await;
    first.authenticate().await;
    second.authenticate().await;
    let one = first.rpc("initialize_schema_context", &request).await;
    let result = &one["result"];
    assert!(
        result["context_handle"]
            .as_str()
            .unwrap()
            .starts_with("context-")
    );
    assert_eq!(result["schema_dump_hash"], request["expected_hash"]);
    let problems = &result["problems"];
    assert_eq!(problems["total_length"], 2);
    assert_eq!(problems["items"][0]["code"], "table_not_found");
    assert_eq!(problems["items"][0]["file_path"], request["schema_dump"]);
    assert!(problems["items"][0]["statement_span"].is_null());
    assert!(problems["items"][0]["probrem_span"].is_null());

    // A historical hit must work without rereading the now-removed dump.
    std::fs::remove_file(request["schema_dump"].as_str().unwrap()).unwrap();
    let two = second.rpc("initialize_schema_context", &request).await;
    assert_ne!(two["result"]["context_handle"], result["context_handle"]);
    assert_eq!(two["result"]["problems"]["items"], problems["items"]);
    let foreign = second
        .rpc(
            "read_collection",
            &::serde_json::json!({
                "collection_handle":problems["handle"], "start_offset":0, "count":null
            }),
        )
        .await;
    assert_eq!(foreign["error"]["data"], "InvalidCollectionHandle");
    let reread = first
        .rpc(
            "read_collection",
            &::serde_json::json!({
                "collection_handle":problems["handle"], "start_offset":0, "count":null
            }),
        )
        .await;
    assert_eq!(&reread["result"], problems);
    first.end_requests().await;
    second.end_requests().await;
    first.finish().await.unwrap();
    second.finish().await.unwrap();
}

// Timed-out first, empty, last, past_end, and oversized cases were removed in
// b3128f415989527c0fd1540cb33ef63f0a67aa20.
#[rstest::rstest]
#[case::exhausted(1026, None, 0)]
#[tokio::test(flavor = "multi_thread")]
async fn collection_rpc_pages_repeatably_and_releases(
    #[case] offset: usize,
    #[case] count: Option<usize>,
    #[case] length: usize,
) {
    let project = SchemaProject::new();
    let request = project.diagnostic_request(513);
    // The test requires a real socket to check wire paging and release.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let initialized = connection.rpc("initialize_schema_context", &request).await;
    let first = &initialized["result"]["problems"];
    assert_eq!(first["items"].as_array().unwrap().len(), 1024);
    assert_eq!(first["total_length"], 1026);
    assert_eq!(first["require_release"], true);
    let read = ::serde_json::json!({"collection_handle":first["handle"], "start_offset":offset, "count":count});
    // Read the tail first, then revisit arbitrary earlier offsets on the same connection.
    let tail = connection
        .rpc(
            "read_collection",
            &::serde_json::json!({
                "collection_handle":first["handle"], "start_offset":1024, "count":null
            }),
        )
        .await;
    assert_eq!(tail["result"]["items"].as_array().unwrap().len(), 2);
    let page = connection.rpc("read_collection", &read).await;
    assert_eq!(page["result"]["items"].as_array().unwrap().len(), length);
    assert_eq!(page["result"]["total_length"], 1026);
    assert_eq!(page["result"]["offset"], offset);
    assert_eq!(connection.rpc("read_collection", &read).await, page);
    let release = ::serde_json::json!({"collection_handle":first["handle"]});
    assert_eq!(
        connection.rpc("release_collection", &release).await["result"],
        ::serde_json::Value::Null
    );
    assert_eq!(
        connection.rpc("read_collection", &read).await["error"]["data"],
        "InvalidCollectionHandle"
    );
    assert_eq!(
        connection.rpc("release_collection", &release).await["error"]["data"],
        "InvalidCollectionHandle"
    );
    assert_eq!(
        connection.rpc("echo", &::serde_json::json!([42])).await["result"],
        ::serde_json::json!([42])
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::missing_hash("expected_hash", ::serde_json::Value::Null, "project requires")]
#[case::unknown_level("schema_level", ::serde_json::json!("invalid"), "unexpected params")]
#[case::bad_hash("expected_hash", ::serde_json::json!("BAD"), "expected_hash")]
#[case::mismatch("expected_hash", ::serde_json::json!("0".repeat(64)), "schema dump changed")]
#[case::unknown_datasource("datasource", ::serde_json::json!("unknown"), "selected datasource")]
#[case::empty_datasource("datasource", ::serde_json::json!(""), "explicit datasource")]
#[case::missing_assets("environments", ::serde_json::json!({}), "SQL_FUN_HOME")]
#[case::unselected_metadata("environments", ::serde_json::json!({"SQL_FUN_HOME":"/nonexistent"}), "SQL_FUN_METADATA_FILE")]
#[case::empty_home("environments", ::serde_json::json!({"SQL_FUN_HOME":"", "SQL_FUN_METADATA_FILE":"x"}), "SQL_FUN_HOME")]
#[tokio::test(flavor = "multi_thread")]
async fn initialization_errors_leave_connection_usable(
    #[case] field: &str,
    #[case] value: ::serde_json::Value,
    #[case] message: &str,
) {
    let project = SchemaProject::new();
    let original = project.rpc_request("CREATE TABLE valid (id int4);");
    let mut invalid = original.clone();
    if value.is_null() {
        invalid.as_object_mut().unwrap().remove(field);
    } else {
        invalid[field] = value;
    }
    // Real sockets exercise recovery after an initialization-level RPC error.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &invalid).await;
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains(message),
        "{response}"
    );
    assert!(response.get("result").is_none());
    let retry = connection.rpc("initialize_schema_context", &original).await;
    assert!(retry["result"]["context_handle"].is_string(), "{retry}");
    assert_eq!(retry["result"]["problems"]["total_length"], 0);
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_initialization_does_not_cancel_server_owned_population() {
    let project = SchemaProject::new();
    let api = api();
    let request = project.rpc_request("CREATE TABLE retained (id int4);");
    let suspended = project.suspend_root(&api).await;
    // Real sockets establish that the cancelled request joined a server-owned load.
    let mut connection = heavy::Connection::start_with_api(4096, 2, &api).await;
    connection.authenticate().await;
    let frame = ::serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize_schema_context","params":request});
    connection
        .send(&format!(
            "{frame}
"
        ))
        .await;
    suspended.wait_for_rpc().await;
    connection.cancel().await;
    suspended.complete().await;
    let mut retry = heavy::Connection::start_with_api(4096, 2, &api).await;
    retry.authenticate().await;
    assert!(
        retry.rpc("initialize_schema_context", &request).await["result"]["context_handle"]
            .is_string()
    );
    retry.end_requests().await;
    retry.finish().await.unwrap();
}

#[rstest::rstest]
#[case::eof(false, false)]
#[case::disconnect(false, true)]
#[case::cancelled(true, false)]
#[tokio::test(flavor = "multi_thread")]
async fn connection_end_invalidates_its_live_collections(
    #[case] cancel: bool,
    #[case] disconnect: bool,
) {
    let project = SchemaProject::new();
    let api = api();
    let request = project.rpc_request("CREATE TABLE retained (id int4);");
    // Real sockets verify teardown and subsequent independent session authority.
    let mut connection = heavy::Connection::start_with_api(4096, 2, &api).await;
    connection.authenticate().await;
    let initialized = connection.rpc("initialize_schema_context", &request).await;
    if cancel {
        connection.cancel().await;
    } else if disconnect {
        connection.disconnect().await;
    } else {
        connection.end_requests().await;
        connection.finish().await.unwrap();
        connection.assert_eof().await;
    }
    let mut replacement = heavy::Connection::start_with_api(4096, 2, &api).await;
    replacement.authenticate().await;
    assert_eq!(
        replacement
            .rpc(
                "read_collection",
                &::serde_json::json!({
                    "collection_handle":initialized["result"]["problems"]["handle"],
                    "start_offset":0, "count":null
                })
            )
            .await["error"]["data"],
        "InvalidCollectionHandle"
    );
    replacement.end_requests().await;
    replacement.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn mismatch_retains_actual_content_for_a_retry_after_dump_removal() {
    let project = SchemaProject::new();
    let original = project.rpc_request("CREATE TABLE retained (id int4);");
    let mut mismatch = original.clone();
    mismatch["expected_hash"] = ::serde_json::json!("0".repeat(64));
    // Real RPC must preserve the service's mismatch publication contract.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    assert!(
        connection
            .rpc("initialize_schema_context", &mismatch)
            .await
            .get("error")
            .is_some()
    );
    std::fs::remove_file(original["schema_dump"].as_str().unwrap()).unwrap();
    let retry = connection.rpc("initialize_schema_context", &original).await;
    assert_eq!(
        retry["result"]["schema_dump_hash"],
        original["expected_hash"]
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn fatal_schema_errors_are_rpc_failures_without_a_context() {
    let project = SchemaProject::new();
    let invalid = project.rpc_request("CREATE TABLE");
    // Real RPC distinguishes fatal parsing failure from recoverable diagnostics.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &invalid).await;
    assert!(response["error"]["data"]["SchemaContextInitializeFailed"].is_string());
    assert!(response.get("result").is_none());
    assert_eq!(
        connection.rpc("echo", &::serde_json::json!([])).await["result"],
        ::serde_json::json!([])
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn extension_diagnostics_follow_explicit_request_order() {
    let project = SchemaProject::new();
    let request = project.request_with_extensions();
    // Real RPC covers extension name/version resolution and ordered serialization.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &request).await;
    let items = response["result"]["problems"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("expected diagnostic page: {response}"));
    let paths: Vec<_> = items
        .iter()
        .map(|item| {
            std::path::Path::new(item["file_path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        })
        .collect();
    assert_eq!(
        paths,
        [
            "second--2.0.sql",
            "second--2.0.sql",
            "first--1.0.sql",
            "first--1.0.sql",
            "rpc.sql",
            "rpc.sql"
        ]
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn retained_statement_ranges_are_not_misreported_as_file_positions() {
    let project = SchemaProject::new();
    let statement = "CREATE DOMAIN broken AS missing;";
    let request = project.rpc_request(statement);
    // Real RPC verifies the diagnostic location contract, using a small catalog.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &request).await;
    let problem = &response["result"]["problems"]["items"][0];
    assert_eq!(problem["code"], "domain_base_type_not_found");
    assert!(problem["statement_span"].is_null());
    assert!(problem["probrem_span"].is_null());
    let range = &problem["statement_relative_span"];
    let offset = usize::try_from(range["offset"].as_u64().unwrap()).unwrap();
    let length = usize::try_from(range["length"].as_u64().unwrap()).unwrap();
    assert_eq!(&statement[offset..offset + length], "missing");
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::table(
    "CREATE TABLE measured (id int4); \
     ALTER TABLE measured ALTER COLUMN id SET STATISTICS 10001; \
     ALTER TABLE measured ALTER COLUMN id SET STATISTICS -2;",
    "alter_column_statistics_reduced",
    "alter_column_statistics_too_low"
)]
#[case::index(
    "CREATE TABLE measured (id int4); \
     CREATE INDEX measured_idx ON measured ((1)); \
     ALTER INDEX measured_idx ALTER COLUMN 1 SET STATISTICS 10001; \
     ALTER INDEX measured_idx ALTER COLUMN 1 SET STATISTICS -2;",
    "index_statistics_clamped",
    "index_invalid_alteration"
)]
#[tokio::test(flavor = "multi_thread")]
async fn statistics_warning_and_rejection_retain_distinct_wire_severities(
    #[case] sql: &str,
    #[case] warning_code: &str,
    #[case] error_code: &str,
) {
    let project = SchemaProject::new();
    let request = project.rpc_request(sql);
    // Real RPC verifies severity survives the diagnostic collection transport.
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &request).await;
    let problems = response["result"]["problems"]["items"].as_array().unwrap();
    let warning = problems
        .iter()
        .find(|problem| problem["code"] == warning_code)
        .unwrap();
    let rejection = problems
        .iter()
        .find(|problem| problem["code"] == error_code)
        .unwrap();
    assert_eq!(warning["severity"], "warning");
    assert_eq!(rejection["severity"], "error");
    let typed: ::sql_fun_server_api::payloads::AnalysisProbrem =
        ::serde_json::from_value(warning.clone()).unwrap();
    assert_eq!(
        *typed.severity(),
        ::sql_fun_server_api::payloads::DiagnosticSeverity::warning()
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn initialization_accepts_core_configuration_transfer() {
    let project = SchemaProject::new();
    let mut request = project.rpc_request("CREATE TABLE chosen_table (id int4);");
    // Core's environment override takes precedence over metadata's public search path.
    request["environments"]["SQL_FUN_PG_SEARCH_PATH"] = ::serde_json::json!("chosen");
    request["environments"]["SQL_FUN_TERM_COLOR"] = ::serde_json::json!("never");
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &request).await;
    assert!(
        response["result"]["context_handle"].is_string(),
        "{response}"
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

mod query_analysis;

#[rstest::rstest]
#[case::builtin("builtin", 0)]
#[case::default_sql("default", 1)]
#[case::extensions("extensions", 3)]
#[case::project("project", 4)]
#[tokio::test(flavor = "multi_thread")]
async fn initialization_rpc_accepts_prefix_and_omits_project_only_requirements(
    #[case] level: &str,
    #[case] layers: usize,
) {
    let project = SchemaProject::new();
    let mut request = project.request_with_extensions();
    request["schema_level"] = ::serde_json::json!(level);
    if level != "project" {
        std::fs::remove_file(request["schema_dump"].as_str().unwrap()).unwrap();
        request.as_object_mut().unwrap().remove("schema_dump");
        request.as_object_mut().unwrap().remove("expected_hash");
    }
    if layers < 3 {
        request["environments"]["SQL_FUN_EXTENSIONS"] = ::serde_json::json!("missing@9.9");
        request["environments"]["SQL_FUN_EXTENSIONS_DIR"] =
            ::serde_json::json!(project.home().join("missing-directory"));
    }
    if layers == 0 {
        std::fs::remove_file(project.home().join("postgres/17/schema.sql")).unwrap();
    }
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    let response = connection.rpc("initialize_schema_context", &request).await;
    assert!(response.get("error").is_none(), "{response}");
    assert!(response["result"]["context_handle"].is_string());
    assert_eq!(
        response["result"]["schema_dump_hash"].is_string(),
        level == "project"
    );
    // Each DROP fixture produces two diagnostics, whereas builtin SQL is empty.
    assert_eq!(
        response["result"]["problems"]["total_length"],
        ::serde_json::json!(layers.saturating_sub(1) * 2)
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}
