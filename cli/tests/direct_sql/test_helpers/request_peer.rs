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
    let peer = ::tokio::spawn(capture_requests(listener, Vec::new(), false, false));
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

/// Exercise schema gating with a real CLI and a deterministic RPC peer.
pub(crate) async fn run_schema(
    options: &[&str],
    severities: &[&str],
    color: &str,
) -> ::testresult::TestResult<(Output, Vec<Value>, String)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let directory = prepare_inputs(listener.local_addr()?.port(), Some(true))?;
    let root = directory.path();
    let diagnostics = severities
        .iter()
        .filter(|severity| **severity != "no_handle" && **severity != "fatal")
        .map(|severity| diagnostic(severity))
        .collect();
    let peer = ::tokio::spawn(capture_requests(
        listener,
        diagnostics,
        severities.contains(&"no_handle"),
        severities.contains(&"fatal"),
    ));
    let output = ::tokio::process::Command::new(cli_executable())
        .current_dir(root)
        .env("USER", root.file_name().unwrap())
        .env("SQL_FUN_SERVER", root.join("unused-server"))
        .env_remove("SQL_FUN_SERVER_COMMAND")
        .env_remove("SQL_FUN_METADATA_FILE")
        .env_remove("SQL_FUN_DATASOURCE")
        .env_remove("RUST_LOG")
        .args([
            "--term-color",
            color,
            "--metadata-file",
            "metadata.toml",
            "--datasource",
            "products",
        ])
        .args(options)
        .args(["--schema-level", "builtin", "--junit-xml", "report.xml"])
        .kill_on_drop(true)
        .output()
        .await?;
    if !root.join("report.xml").exists() {
        peer.abort();
        panic!(
            "CLI did not write report: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let (queries, _) = peer.await?;
    let report = std::fs::read_to_string(root.join("report.xml"))?;
    Ok((output, queries, report))
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
    std::fs::write(
        root.join("diagnostic-cases.json"),
        r#"[{"name":"expected","sql":"warning case","expected":["parameter_constraint"]},{"name":"expected-error","sql":"error case","expected":["parameter_constraint"]},{"name":"unexpected","sql":"warning case","expected":[]},{"name":"later","sql":"SELECT 2","expected":[]}]"#,
    )?;
    std::fs::write(
        root.join("error-cases.json"),
        r#"[{"name":"rpc-error","sql":"api_error","expected":[]}]"#,
    )?;
    std::fs::write(
        root.join("multiple-cases.json"),
        r#"[{"name":"multiple","sql":"mixed case","expected":[]}]"#,
    )?;
    Ok(directory)
}

async fn capture_requests(
    listener: TcpListener,
    diagnostics: Vec<Value>,
    no_handle: bool,
    fatal: bool,
) -> (Vec<Value>, Option<Value>) {
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
        if (fatal && request["method"] == "initialize_schema_context")
            || (request["method"] == "analyze_query"
                && request["params"]["query"]["Text"] == "api_error")
        {
            if request["method"] == "analyze_query" {
                requests.push(request["params"].clone());
            }
            let response = ::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"injected internal failure"}});
            stream
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
            continue;
        }
        let result = match request["method"].as_str().unwrap() {
            "initialize" => ::serde_json::json!("OK"),
            "initialize_schema_context" => {
                initialization = Some(request["params"].clone());
                ::serde_json::json!({
                    "context_handle":if no_handle { None } else { Some("test") }, "schema_dump_hash":null, "problems":page(&diagnostics)
                })
            }
            "analyze_query" => {
                let query = request["params"]["query"]["Text"].as_str().unwrap();
                let severities = match query {
                    "warning case" => vec![Some("warning")],
                    "error case" => vec![Some("error")],
                    "mixed case" => vec![Some("warning"), Some("error")],
                    "advice case" => vec![Some("advice")],
                    _ => vec![None],
                };
                let items = severities.into_iter().map(statement).collect::<Vec<_>>();
                requests.push(request["params"].clone());
                ::serde_json::json!({"queries":page(&items)})
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

fn statement(severity: Option<&str>) -> Value {
    let diagnostics = if let Some(severity) = severity {
        vec![diagnostic(severity)]
    } else {
        Vec::new()
    };
    ::serde_json::json!({
        "query_span":null, "query_kind":"Select", "execute_sql":"SELECT 1",
        "complete":false, "probrems":page(&diagnostics), "parameters":page(&[]),
        "result_columns":page(&[]), "column_access":page(&[])
    })
}

fn diagnostic(severity: &str) -> Value {
    let problem = ::sql_fun_core::analysis_problem::AnalysisProblem::parameter_constraint(
        "unresolved parameter",
    );
    let severity = match severity {
        "warning" => ::sql_fun_client::payloads::DiagnosticSeverity::warning(),
        "error" => ::sql_fun_client::payloads::DiagnosticSeverity::error(),
        _ => ::sql_fun_client::payloads::DiagnosticSeverity::advice(),
    };
    let diagnostic = ::sql_fun_client::payloads::AnalysisProbrem::new(
        problem.code(),
        severity,
        std::path::Path::new("schema"),
        None,
    )
    .with_semantic_problem(&problem);
    ::serde_json::to_value(diagnostic).unwrap()
}
