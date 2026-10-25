//! Authenticated JSON Lines CLI integration tests against the actual server.
#[path = "api_helpers.rs"]
mod test_helpers;

use self::test_helpers::{heavy, responses};
use std::time::Duration;

use ::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const ECHO: &str = "{\"method\":\"echo\",\"params\":{\"nested\":[null,true,{\"n\":42}]}}\n";

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn streams_before_eof_and_preserves_server_on_reuse() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut child = server.command().spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(ECHO.as_bytes()).await?;
    let mut first = String::new();
    ::tokio::time::timeout(Duration::from_secs(45), stdout.read_line(&mut first)).await??;
    let first: ::serde_json::Value = ::serde_json::from_str(&first)?;
    assert_eq!(
        first["result"],
        ::serde_json::json!({"nested":[null,true,{"n":42}]})
    );
    let registration = server.registration()?;
    // Submit multiple requests before consuming any further responses.
    for n in 0..12 {
        stdin
            .write_all(
                format!(
                    "{{\"jsonrpc\":\"2.0\",\"method\":\"echo\",\"params\":[{n},{{\"a\":null}}]}}\n"
                )
                .as_bytes(),
            )
            .await?;
    }
    drop(stdin);
    let mut ids = std::collections::HashSet::from([first["id"].as_u64().unwrap()]);
    let mut values = std::collections::HashSet::new();
    loop {
        let mut line = String::new();
        if ::tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line)).await?? == 0
        {
            break;
        }
        let response: ::serde_json::Value = ::serde_json::from_str(&line)?;
        assert_eq!(response["jsonrpc"], "2.0");
        assert!(ids.insert(response["id"].as_u64().unwrap()));
        let n = response["result"][0].as_u64().unwrap();
        assert_eq!(response["result"][1], ::serde_json::json!({"a":null}));
        // Client handshake uses ID 0; each subsequent submitted request advances ID.
        assert_eq!(
            response["id"].as_u64().unwrap(),
            first["id"].as_u64().unwrap() + n + 1
        );
        values.insert(n);
    }
    assert_eq!(values.len(), 12);
    assert!(
        ::tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await??
            .success()
    );
    let mut reuse = server.command();
    reuse.args(["--server-command", "exit 99"]);
    let output = heavy::ApiServer::run_command(reuse, ECHO).await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(responses(&output).len(), 1);
    heavy::verify_query_analysis_session(&server).await?;
    assert_eq!(registration, server.registration()?);
    Ok(())
}

#[rstest::rstest]
#[case::malformed("{")]
#[case::non_object("[]")]
#[case::id_null(r#"{"method":"echo","params":{},"id":null}"#)]
#[case::id_number(r#"{"method":"echo","params":{},"id":2}"#)]
#[case::version_old(r#"{"jsonrpc":"1.0","method":"echo","params":{}}"#)]
#[case::version_null(r#"{"jsonrpc":null,"method":"echo","params":{}}"#)]
#[case::version_number(r#"{"jsonrpc":2,"method":"echo","params":{}}"#)]
#[case::unknown(r#"{"method":"no_such_method","params":{}}"#)]
#[case::wrong_params(r#"{"method":"echo","params":null}"#)]
#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn invalid_later_line_drains_accepted_requests(
    #[case] invalid: &str,
) -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let input = format!("{ECHO}{invalid}\n{ECHO}");
    let output = server.run(&input).await?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("input line 2"));
    let replies = responses(&output);
    assert_eq!(replies.len(), 1);
    assert_eq!(
        replies[0]["result"],
        ::serde_json::json!({"nested":[null,true,{"n":42}]})
    );
    assert!(server.run(ECHO).await?.status.success());
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn cargo_startup_cli_precedence_and_logs_on_stderr() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut command = server.command();
    let launch = if cfg!(feature = "sqlite") {
        "cargo run -p sql-fun-server --features sqlite"
    } else {
        "cargo run -p sql-fun-server"
    };
    command
        .env("SQL_FUN_SERVER_COMMAND", "exit 99")
        .args(["--server-command", launch])
        .env("USER", "invalid/ambient")
        .args(["--user", server.user()])
        .env("RUST_LOG", "info");
    let output = heavy::ApiServer::run_command(command, ECHO).await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(responses(&output).len(), 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("echo"));
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn path_resolution_and_server_error_envelope() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut command = server.command();
    command
        .env_remove("SQL_FUN_SERVER")
        .env("PATH", server.executable().parent().unwrap());
    let output = heavy::ApiServer::run_command(
        command,
        concat!(
            r#"{"method":"release_collection","params":{"collection_handle":"unused"}}"#,
            "\n"
        ),
    )
    .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies = responses(&output);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["jsonrpc"], "2.0");
    assert!(replies[0]["id"].is_number());
    assert!(replies[0]["error"]["code"].is_i64());
    assert_eq!(replies[0]["error"]["message"], "invalid collection handle");
    assert_eq!(replies[0]["error"]["data"], "InvalidCollectionHandle");
    assert!(replies[0].get("result").is_none());
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn broken_stdout_is_reported_while_stdin_stays_open() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut child = server.command().spawn()?;
    drop(child.stdout.take());
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(ECHO.as_bytes()).await?;
    let output =
        ::tokio::time::timeout(Duration::from_secs(45), child.wait_with_output()).await??;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("api stdout"));
    drop(stdin);
    assert!(server.run(ECHO).await?.status.success());
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn transport_failure_exits_while_stdin_is_open() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut child = server.command().spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(ECHO.as_bytes()).await?;
    let mut line = String::new();
    ::tokio::time::timeout(Duration::from_secs(45), stdout.read_line(&mut line)).await??;
    server.stop_owned_server()?;
    let output =
        ::tokio::time::timeout(Duration::from_secs(10), child.wait_with_output()).await??;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("api transport"));
    drop(stdin);
    Ok(())
}

#[rstest::rstest]
#[case::missing_user("USER", "--user")]
#[case::invalid_executable("SQL_FUN_SERVER", "SQL_FUN_SERVER")]
#[tokio::test]
async fn missing_choices_fail_explicitly(
    #[case] setting: &str,
    #[case] diagnostic: &str,
) -> ::testresult::TestResult {
    let mut command = ::tokio::process::Command::new(
        std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"),
    );
    command
        .arg("api")
        .env("USER", "unit-test")
        .env("SQL_FUN_SERVER", "")
        .env_remove("SQL_FUN_SERVER_COMMAND");
    if setting == "USER" {
        command.env_remove("USER");
    }
    let output = command.output().await?;
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn environment_launch_and_explicit_reference_do_not_discover_metadata()
-> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let mut command = server.command();
    // A deliberately unusable ambient metadata path must not be inspected.
    command
        .env(
            "SQL_FUN_METADATA_FILE",
            "/nonexistent/unselected/Cargo.toml",
        )
        .env(
            "SQL_FUN_SERVER_COMMAND",
            format!("'{}'", server.executable().display()),
        );
    let output = heavy::ApiServer::run_command(command, ECHO).await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(responses(&output).len(), 1);
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn renders_server_diagnostic_from_retained_source_after_file_changes()
-> ::testresult::TestResult {
    use ::clap::Parser;

    // A real server and client verify source provenance across the transport.
    let server = heavy::ApiServer::new()?;
    let schema = test_helpers::schema::SchemaFixture::with_sql(
        "CREATE SCHEMA retained; CREATE SCHEMA retained;",
    )?;
    let context = server.initialize_schema(&schema).await?;
    test_helpers::overwrite_schema_source(&schema, "-- changed after analysis")?;
    let problem = context
        .problems()
        .items()
        .first()
        .expect("duplicate schema diagnostic");
    let presentation = problem.presentation().as_ref().expect("retained statement");
    let args = ::sql_fun_core::SqlFunArgs::try_parse_from(["sql-fun", "--term-color", "never"])?;
    let mut client = server.connect().await?;
    let output = ::sql_fun_cli::render_diagnostic(&mut client, presentation, &args).await?;
    client.finish().await?;
    assert_eq!(problem.code(), "conflicting_creation");
    assert_eq!(presentation.source(), " CREATE SCHEMA retained");
    assert!(output.contains(presentation.source()), "{output}");
    assert!(!output.contains("changed after analysis"));
    assert!(!output.contains("\x1b["));
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn tokenization_is_available_without_schema_or_project_discovery() -> ::testresult::TestResult
{
    let server = heavy::ApiServer::new()?;
    let mut bridge = heavy::BridgeSession::start(&server)?;
    let source = "SELECT '日本語'; -- comment\n";
    let reply = bridge
        .rpc(
            "tokenize_sql",
            &::serde_json::json!({"source":source, "dialect":"postgres"}),
        )
        .await?;
    let tokens = &reply["result"];
    assert!(tokens["items"].as_array().unwrap().len() > 1, "{reply}");
    assert_eq!(tokens["items"][0]["kind"], "keyword");
    assert_eq!(tokens["items"][0]["start"], 0);
    assert_eq!(
        tokens["items"].as_array().unwrap().last().unwrap()["end"],
        source.len()
    );
    let released = bridge
        .rpc(
            "release_collection",
            &::serde_json::json!({"collection_handle":tokens["handle"]}),
        )
        .await?;
    assert!(released.get("error").is_none(), "{released}");
    bridge.finish().await?;
    Ok(())
}
