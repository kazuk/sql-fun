use crate::test_helpers::heavy;

#[tokio::test(flavor = "multi_thread")]
async fn buffered_requests_survive_authentication_and_drain_on_eof() {
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.send(concat!(
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"sql_fun_token":"session-test-token"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":{"nested":[null,true,42]}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"echo","params":[1,2]}"#,
        "\n",
    )).await;
    connection.end_requests().await;
    assert_eq!(
        connection.response().await,
        ::serde_json::json!({"jsonrpc":"2.0","id":0,"result":"OK"})
    );
    assert_eq!(
        connection.response().await,
        ::serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"nested":[null,true,42]}})
    );
    assert_eq!(
        connection.response().await,
        ::serde_json::json!({"jsonrpc":"2.0","id":2,"result":[1,2]})
    );
    connection.finish().await.unwrap();
    connection.assert_eof().await;
}

#[rstest::rstest]
#[case::unknown_method(r#"{"jsonrpc":"2.0","id":1,"method":"unknown","params":{}}"#)]
#[case::invalid_params(r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":42}"#)]
#[tokio::test(flavor = "multi_thread")]
async fn rpc_error_allows_next_request(#[case] request: &str) {
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    connection.send(&format!("{request}\n")).await;
    let error = connection.response().await;
    assert_eq!(error["id"], 1);
    assert_eq!(error["jsonrpc"], "2.0");
    assert!(error.get("error").is_some());
    assert!(error.get("result").is_none());
    connection
        .send(concat!(
            r#"{"jsonrpc":"2.0","id":2,"method":"echo","params":[]}"#,
            "\n"
        ))
        .await;
    assert_eq!(
        connection.response().await,
        ::serde_json::json!({"jsonrpc":"2.0","id":2,"result":[]})
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::non_initialize(
    r#"{"jsonrpc":"2.0","id":0,"method":"echo","params":{}}"#,
    "protocol failed"
)]
#[case::missing_token(
    r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#,
    "invalid token"
)]
#[case::wrong_token(
    r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"sql_fun_token":"wrong"}}"#,
    "invalid token"
)]
#[case::eof("", "server can not receive request (IO or JSON)")]
#[tokio::test(flavor = "multi_thread")]
async fn rejected_authentication_has_no_acknowledgement(
    #[case] request: &str,
    #[case] reason: &str,
) {
    let mut connection = heavy::Connection::start(4096, 2).await;
    if !request.is_empty() {
        connection.send(&format!("{request}\n")).await;
    }
    connection.end_requests().await;
    assert_eq!(
        connection.finish().await.unwrap_err().to_string(),
        format!("Authentication Failed: {reason}")
    );
    connection.assert_no_response().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn authentication_timeout_has_no_acknowledgement() {
    let mut connection = heavy::Connection::start(4096, 0).await;
    assert_eq!(
        connection.finish().await.unwrap_err().to_string(),
        "Authentication Failed: timeout"
    );
    connection.assert_no_response().await;
}

#[rstest::rstest]
#[case::malformed("not-json\n")]
#[case::truncated(r#"{"jsonrpc":"#)]
#[case::missing_newline(r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":[]}"#)]
#[case::invalid_envelope("{}\n")]
#[tokio::test(flavor = "multi_thread")]
async fn invalid_authenticated_input_terminates_handler(#[case] input: &str) {
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    connection.send(input).await;
    connection.end_requests().await;
    assert!(connection.finish().await.is_err());
    connection.assert_no_response().await;
}

#[rstest::rstest]
#[case::allowed(4096, true)]
#[case::rejected(1024, false)]
#[tokio::test(flavor = "multi_thread")]
async fn authenticated_request_uses_configured_limit(#[case] limit: usize, #[case] allowed: bool) {
    let mut connection = heavy::Connection::start(limit, 2).await;
    connection.authenticate().await;
    let params = ::serde_json::json!({"value": "x".repeat(1500)});
    let request = ::serde_json::json!({"jsonrpc":"2.0","id":1,"method":"echo","params":params});
    connection.send(&format!("{request}\n")).await;
    connection.end_requests().await;
    if allowed {
        assert_eq!(
            connection.response().await,
            ::serde_json::json!({"jsonrpc":"2.0","id":1,"result":params})
        );
        connection.finish().await.unwrap();
        connection.assert_eof().await;
    } else {
        assert!(
            connection
                .finish()
                .await
                .unwrap_err()
                .to_string()
                .contains("request length")
        );
        connection.assert_no_response().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_disconnect_finishes_handler() {
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    connection.end_requests().await;
    connection.finish().await.unwrap();
    connection.assert_eof().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancellation_terminates_authenticated_connection_task() {
    let mut connection = heavy::Connection::start(4096, 2).await;
    connection.authenticate().await;
    connection.cancel().await;
    // Await connection closure, allowing the existing reader's cancellation
    // guard and its background poller to release the read half asynchronously.
    connection.assert_eof().await;
}
