use std::process::Output;

use ::serde_json::Value;
use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

/// Capture requests from the actual CLI without starting a service or parsing a catalog.
/// The registration, metadata, and SQL inputs are all owned by this fixture.
pub(crate) async fn run(options: &[&str]) -> ::testresult::TestResult<(Output, Vec<Value>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let directory = prepare_inputs(listener.local_addr()?.port())?;
    let root = directory.path();
    let peer = ::tokio::spawn(capture_requests(listener));
    let output = ::tokio::process::Command::new(super::api::executable::cli_executable())
        .current_dir(root)
        .env("USER", root.file_name().unwrap())
        .env("SQL_FUN_SERVER", root.join("unused-server"))
        .env_remove("SQL_FUN_SERVER_COMMAND")
        .env_remove("SQL_FUN_METADATA_FILE")
        .env_remove("SQL_FUN_DATASOURCE")
        .env_remove("RUST_LOG")
        .args([
            "--term-color",
            "never",
            "--metadata-file",
            "metadata.toml",
            "--datasource",
            "products",
        ])
        .args(options)
        .args(["--schema-level", "builtin"])
        .kill_on_drop(true)
        .output()
        .await?;
    Ok((output, peer.await?))
}

fn prepare_inputs(port: u16) -> ::testresult::TestResult<::tempfile::TempDir> {
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
        r#"[package.metadata.database.products]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "products-connector"
"#,
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

async fn capture_requests(listener: TcpListener) -> Vec<Value> {
    let (stream, _) = listener.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    let mut requests = Vec::new();
    loop {
        let mut line = String::new();
        if stream.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        let request: Value = ::serde_json::from_str(&line).unwrap();
        let result = match request["method"].as_str().unwrap() {
            "initialize" => ::serde_json::json!("OK"),
            "initialize_schema_context" => ::serde_json::json!({
                "context_handle":"test", "schema_dump_hash":null, "problems":page(&[])
            }),
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
    requests
}

fn page(items: &[Value]) -> Value {
    ::serde_json::json!({"handle":"test", "total_length":items.len(), "offset":0, "require_release":false, "items":items})
}
