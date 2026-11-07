//! Actual-server lifecycle coverage in the maintained workspace test suite.

mod test_helpers;

use self::test_helpers::{heavy, registration::RegistrationFixture};
use ::sql_fun_client::connect;
use std::time::{Duration, Instant};

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn launches_authenticates_and_reuses_without_executing_override() -> ::testresult::TestResult
{
    let fixture = heavy::ServerFixture::new()?;
    let mut first = connect(&fixture.options()).await?;
    let address = first.peer_addr()?;
    let before = fixture.registration()?;
    fixture.assert_detached()?;
    // Exercise the post-authentication result/error envelope against the server.
    let (sent, received) = ::tokio::sync::oneshot::channel();
    first
        .invoke_method(
            "unknown_method",
            &::serde_json::json!({}),
            move |response| async move {
                let _ = sent.send(response.into_result().is_err());
                Ok(())
            },
        )
        .await?;
    first.finish().await?;
    assert!(received.await?);
    let second = connect(&fixture.with_command(Some("exit 97"), Duration::from_secs(3))).await?;
    assert_eq!(address, second.peer_addr()?);
    assert_eq!(before, fixture.registration()?);
    drop(second);
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn concurrent_fresh_clients_find_one_primary() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    let options = fixture.options();
    let (first, second) = ::tokio::join!(connect(&options), connect(&options));
    assert_eq!(first?.peer_addr()?, second?.peer_addr()?);
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn cargo_command_launch_uses_prebuilt_actual_server() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    let command = if cfg!(feature = "sqlite") {
        "cargo run -p sql-fun-server --features sqlite"
    } else {
        "cargo run -p sql-fun-server"
    };
    let client = connect(&fixture.with_command(Some(command), Duration::from_secs(10))).await?;
    assert!(client.peer_addr()?.ip().is_loopback());
    Ok(())
}

#[tokio::test]
async fn unverifiable_registration_is_preserved_without_launch() -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    let record = fixture.preserve_unusable_record()?;
    let start = Instant::now();
    let error = connect(&fixture.options(Duration::from_millis(150)))
        .await
        .unwrap_err();
    assert!(!error.is_timeout());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(error.to_string().contains("cannot verify"));
    assert!(error.to_string().contains("TCP connect"));
    assert!(!error.to_string().contains("fixture-secret"));
    assert_eq!(record, std::fs::read(fixture.path())?);
    Ok(())
}

#[rstest::rstest]
#[case::malformed(b"{\"token\":\"fixture-secret\",", "malformed")]
#[case::missing(b"{\"pid\":1,\"token\":\"fixture-secret\"}", "malformed")]
#[case::zero_pid(b"{\"pid\":0,\"port\":1234,\"token\":\"fixture-secret\"}", "invalid")]
#[case::zero_port(b"{\"pid\":1,\"port\":0,\"token\":\"fixture-secret\"}", "invalid")]
#[case::empty_token(b"{\"pid\":1,\"port\":1234,\"token\":\"\"}", "invalid")]
#[tokio::test]
async fn invalid_registration_is_preserved(
    #[case] record: &[u8],
    #[case] diagnostic: &str,
) -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    fixture.write(record)?;
    let error = connect(&fixture.options(Duration::from_secs(1)))
        .await
        .unwrap_err();
    assert!(!error.is_timeout());
    assert!(error.to_string().contains(diagnostic));
    assert!(!error.to_string().contains("fixture-secret"));
    assert_eq!(record, std::fs::read(fixture.path())?);
    Ok(())
}

#[tokio::test]
async fn invalid_options_do_not_read_registration_or_execute_command() -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    fixture.write(b"malformed")?;
    let error = connect(&fixture.options(Duration::ZERO)).await.unwrap_err();
    assert!(error.to_string().starts_with("invalid input:"));
    assert_eq!(std::fs::read(fixture.path())?, b"malformed");
    Ok(())
}

#[tokio::test]
async fn spawn_failure_is_distinct_from_timeout() -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    let error = connect(&fixture.spawn_failure_options()).await.unwrap_err();
    assert!(!error.is_timeout());
    assert!(error.to_string().contains("cannot spawn"));
    assert!(!fixture.path().exists());
    Ok(())
}

#[tokio::test]
async fn broken_shell_command_retains_launch_failure_at_deadline() -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    let error = connect(&fixture.options(Duration::from_millis(200)))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert!(error.to_string().contains("launch exited"));
    assert!(!fixture.path().exists());
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn rereads_changed_registration_within_original_deadline() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    let first = connect(&fixture.options()).await?;
    let address = first.peer_addr()?;
    drop(first);
    let healthy = fixture.registration()?;
    fixture.preserve_unusable_record()?;
    let restore = async {
        ::tokio::time::sleep(Duration::from_millis(80)).await;
        fixture.persist_registration(&healthy).unwrap();
    };
    let options = fixture.with_command(Some("exit 97"), Duration::from_secs(2));
    let (client, ()) = ::tokio::join!(connect(&options), restore);
    assert_eq!(client?.peer_addr()?, address);
    assert_eq!(healthy, fixture.registration()?);
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn streams_multiple_requests_and_drains_actual_server() -> ::testresult::TestResult {
    // The actual server is required to verify authenticated half-close semantics.
    let fixture = heavy::ServerFixture::new()?;
    let mut client = connect(&fixture.options()).await?;
    let (sent, mut received) = ::tokio::sync::mpsc::unbounded_channel();
    for number in 0..12 {
        let sent = sent.clone();
        client
            .invoke_method(
                "unknown_method",
                &::serde_json::json!({"number":number}),
                move |response| async move {
                    let id = response.id();
                    assert!(response.into_result().is_err());
                    let _ = sent.send(id);
                    Ok(())
                },
            )
            .await?;
    }
    ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await??;
    let mut ids = Vec::new();
    for _ in 0..12 {
        ids.push(received.recv().await.unwrap());
    }
    ids.sort_unstable();
    assert_eq!(ids, (1..=12).collect::<Vec<_>>());
    let reused = connect(&fixture.options()).await?;
    reused.finish().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn typed_response_completes_real_session_collection_and_releases() -> ::testresult::TestResult
{
    // The actual server verifies authenticated handle lifetime and real bounded paging.
    let fixture = heavy::ServerFixture::new()?;
    let schema = test_helpers::schema::SchemaFixture::new()?;
    let mut client = connect(&fixture.options()).await?;
    let (sent, mut received) = ::tokio::sync::mpsc::unbounded_channel();
    for _ in 0..8 {
        let sent = sent.clone();
        client
            .invoke_typed(
                "initialize_schema_context",
                schema.request(),
                move |response: Result<ObservedContext, _>| async move {
                    let value = response.unwrap();
                    assert!(value.context.problems().is_completed());
                    assert_eq!(value.context.problems().items().len(), 1026);
                    let _ = sent.send(value.handle);
                    Ok(())
                },
            )
            .await?;
    }
    drop(sent);
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(
            ::tokio::time::timeout(Duration::from_secs(20), received.recv())
                .await?
                .unwrap(),
        );
    }
    let (sent, mut received) = ::tokio::sync::mpsc::unbounded_channel();
    for handle in handles {
        let sent = sent.clone();
        client
            .invoke_method(
                "read_collection",
                &::serde_json::json!({"collection_handle":handle, "start_offset":0, "count":1}),
                move |response| async move {
                    let _ = sent.send(response.into_result().is_err());
                    Ok(())
                },
            )
            .await?;
    }
    drop(sent);
    ::tokio::time::timeout(Duration::from_secs(10), client.finish()).await??;
    for _ in 0..8 {
        assert!(received.recv().await.unwrap());
    }

    Ok(())
}

// Preserve the issued handle before completion solely to verify release afterward.
#[derive(::serde::Deserialize)]
#[serde(from = "sql_fun_client::payloads::SchemaContext")]
struct ObservedContext {
    context: ::sql_fun_client::payloads::SchemaContext,
    handle: ::sql_fun_client::payloads::CollectionHandle,
}
impl From<::sql_fun_client::payloads::SchemaContext> for ObservedContext {
    fn from(context: ::sql_fun_client::payloads::SchemaContext) -> Self {
        let handle = context.problems().handle().unwrap().clone();
        Self { context, handle }
    }
}
impl ::sql_fun_client::CompleteResponse for ObservedContext {
    fn register_collections(&self, context: &mut ::sql_fun_client::CompletionContext) {
        self.context.register_collections(context);
    }
    fn complete<'a>(
        &'a mut self,
        context: &'a mut ::sql_fun_client::CompletionContext,
    ) -> ::sql_fun_client::CompletionFuture<'a> {
        self.context.complete(context)
    }
}
