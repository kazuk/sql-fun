use crate::{
    session_rpc_tests::query_analysis::test_helpers::{query_project, rewrite_request},
    test_helpers::heavy,
};
use ::serde_json::Value;

/// Analyze one immutable schema snapshot through a fresh RPC connection.
/// Each call starts a socket handler and a small synthetic catalog, rather than
/// parsing the full builtin catalog. Independent installations represent separate
/// database environments without depending on file-watcher notification timing.
pub(crate) async fn analyze_schema_snapshot(
    schema: &str,
    sql: &str,
    expand: Option<bool>,
) -> Value {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &rewrite_request(&project, schema, expand),
        )
        .await;
    assert!(
        initialized["result"]["context_handle"].is_string(),
        "{initialized}"
    );
    let response = connection
        .rpc(
            "analyze_query",
            &::serde_json::json!({
                "context_handle": initialized["result"]["context_handle"],
                "query": {"Text": sql}
            }),
        )
        .await;
    assert!(response.get("result").is_some(), "{response}");
    let item = response["result"]["queries"]["items"][0].clone();
    connection.end_requests().await;
    connection.finish().await.unwrap();
    item
}
