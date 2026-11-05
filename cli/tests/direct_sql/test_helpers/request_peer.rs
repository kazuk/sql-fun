use crate::test_helpers::api::executable::cli_executable;
use std::process::Output;

use ::serde_json::Value;
use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

/// Capture requests from the actual CLI without starting a service or parsing a catalog.
/// The registration, metadata, and SQL inputs are all owned by this fixture.
pub(crate) async fn run(options: &[&str]) -> ::testresult::TestResult<(Output, Vec<Value>)> {
    let (output, queries, _) = run_selection(options, Some("products"), None, Some(true)).await?;
    Ok((output, queries))
}

/// Run direct SQL with controlled datasource inputs and capture its initialization request.
pub(crate) async fn run_selection(
    options: &[&str],
    cli_datasource: Option<&str>,
    env_datasource: Option<&str>,
    valid_default: Option<bool>,
) -> ::testresult::TestResult<(Output, Vec<Value>, Option<Value>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let directory = prepare_inputs(listener.local_addr()?.port(), valid_default)?;
    let root = directory.path();
    let peer = ::tokio::spawn(capture_requests(listener));
    let mut command = ::tokio::process::Command::new(cli_executable());
    command
        .current_dir(root)
        .env("USER", root.file_name().unwrap())
        .env("SQL_FUN_SERVER", root.join("unused-server"))
        .env_remove("SQL_FUN_SERVER_COMMAND")
        .env_remove("SQL_FUN_METADATA_FILE")
        .env_remove("SQL_FUN_DATASOURCE")
        .env_remove("RUST_LOG")
        .args(["--term-color", "never", "--metadata-file", "metadata.toml"]);
    if let Some(name) = cli_datasource {
        command.args(["--datasource", name]);
    }
    if let Some(name) = env_datasource {
        command.env("SQL_FUN_DATASOURCE", name);
    }
    let output = command
        .args(options)
        .args(["--schema-level", "builtin"])
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        peer.abort();
        return Ok((output, Vec::new(), None));
    }
    let (queries, initialization) = peer.await?;
    Ok((output, queries, initialization))
}

fn prepare_inputs(
    port: u16,
    valid_default: Option<bool>,
) -> ::testresult::TestResult<::tempfile::TempDir> {
    let directory = ::tempfile::Builder::new()
        .prefix("sql-fun-request-")
        .tempdir_in("/tmp")?;
    let root = directory.path();
    std::fs::write(
        root.join("sql-fun-server.pid"),
        ::serde_json::to_vec(&::serde_json::json!({
            "pid":std::process::id(), "port":port, "token":"test-token"
        }))?,
    )?;
    std::fs::write(
        root.join("metadata.toml"),
        format!(
            r#"[package.metadata.database.products]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "products-connector"
{}
"#,
            match valid_default {
                Some(true) =>
                    r#"[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "default-connector""#,
                Some(false) =>
                    r#"[package.metadata.database.default]
engine = "PostgreSQL"
dialect = 42
version = "17"
connector = "default-connector""#,
                None => "",
            }
        ),
    )?;
    std::fs::write(
        root.join("queries.json"),
        r#"{"queries":["SELECT 1","SELECT 2"],"expected":[[],[]]}"#,
    )?;
    std::fs::write(
        root.join("cases.json"),
        r#"[{"name":"first","sql":"SELECT 1","expected":[]},{"name":"second","sql":"SELECT 2","expected":[]}]"#,
    )?;
    Ok(directory)
}

async fn capture_requests(listener: TcpListener) -> (Vec<Value>, Option<Value>) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    let mut requests = Vec::new();
    let mut initialization = None;
    loop {
        let mut line = String::new();
        if stream.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        let request: Value = ::serde_json::from_str(&line).unwrap();
        let result = match request["method"].as_str().unwrap() {
            "initialize" => ::serde_json::json!("OK"),
            "initialize_schema_context" => {
                initialization = Some(request["params"].clone());
                ::serde_json::json!({
                    "context_handle":"test", "schema_dump_hash":null, "problems":page(&[])
                })
            }
            "analyze_query" => {
                let item = ::serde_json::json!({
                    "query_span":null, "query_kind":"Select", "execute_sql":null,
                    "complete":true, "probrems":page(&[]), "parameters":page(&[]),
                    "result_columns":page(&[]), "column_access":page(&[])
                });
                requests.push(request["params"].clone());
                ::serde_json::json!({"queries":page(&[item])})
            }
            method => panic!("unexpected method {method}"),
        };
        let response = ::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result});
        stream
            .get_mut()
            .write_all(format!("{response}\n").as_bytes())
            .await
            .unwrap();
    }
    (requests, initialization)
}

fn page(items: &[Value]) -> Value {
    ::serde_json::json!({"handle":"test", "total_length":items.len(), "offset":0, "require_release":false, "items":items})
}
