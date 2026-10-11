//! Actual-server lifecycle coverage in the maintained workspace test suite.

mod test_helpers;

use self::test_helpers::{heavy, registration::RegistrationFixture};
use ::sql_fun_client::connect;
use std::time::{Duration, Instant};

#[tokio::test]
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
async fn concurrent_fresh_clients_find_one_primary() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    let options = fixture.options();
    let (first, second) = ::tokio::join!(connect(&options), connect(&options));
    assert_eq!(first?.peer_addr()?, second?.peer_addr()?);
    Ok(())
}

#[tokio::test]
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
async fn unusable_registration_times_out_without_mutation_or_launch() -> ::testresult::TestResult {
    let fixture = RegistrationFixture::new()?;
    let record = fixture.preserve_unusable_record()?;
    let start = Instant::now();
    let error = connect(&fixture.options(Duration::from_millis(150)))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert!(start.elapsed() < Duration::from_secs(2));
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
async fn rejected_token_preserves_actual_server_and_registration() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    drop(connect(&fixture.options()).await?);
    let record = fixture.change_token()?;
    let error = connect(&fixture.with_command(Some("exit 97"), Duration::from_millis(150)))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert!(error.to_string().contains("initialize"));
    assert!(!error.to_string().contains("mismatched-fixture-secret"));
    assert_eq!(record, fixture.registration()?);
    Ok(())
}

#[tokio::test]
async fn stalled_actual_server_is_preserved_with_bounded_deadline() -> ::testresult::TestResult {
    let fixture = heavy::ServerFixture::new()?;
    drop(connect(&fixture.options()).await?);
    let record = fixture.registration()?;
    fixture.pause()?;
    let start = Instant::now();
    let error = connect(&fixture.with_command(None, Duration::from_millis(150)))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(
        error
            .to_string()
            .contains("initialize write/read timed out")
    );
    assert_eq!(record, fixture.registration()?);
    Ok(())
}

#[tokio::test]
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
