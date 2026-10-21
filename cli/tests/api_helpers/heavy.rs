// Reuse the real-client lifecycle fixture, including isolated namespaces and
// guarded cleanup. No production test API is needed.
#[allow(dead_code)]
#[path = "../../../sql-fun-client/tests/test_helpers/heavy.rs"]
mod lifecycle;

use std::{path::Path, process::Stdio, time::Duration};

use ::tokio::io::AsyncWriteExt;

/// Prebuilds a real server for each test process and owns one isolated namespace.
/// Cargo's cache is shared; external service initialization is not shared across
/// tests. This setup verifies authentication, streaming, and server persistence.
pub struct ApiServer(lifecycle::ServerFixture);

impl ApiServer {
    pub fn new() -> ::testresult::TestResult<Self> {
        Ok(Self(lifecycle::ServerFixture::new()?))
    }

    pub fn command(&self) -> ::tokio::process::Command {
        let mut command = ::tokio::process::Command::new(env!("CARGO_BIN_EXE_sql-fun"));
        command
            .args(["api"])
            .env("USER", self.0.user())
            .env("SQL_FUN_SERVER", self.0.executable())
            .env_remove("SQL_FUN_SERVER_COMMAND")
            .env_remove("SQL_FUN_METADATA_FILE")
            .env_remove("RUST_LOG")
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    /// Run schema initialization through the CLI's real client/server transport.
    pub async fn initialize_schema(
        &self,
        schema: &super::schema::SchemaFixture,
    ) -> ::testresult::TestResult<::sql_fun_server_api::payloads::SchemaContext> {
        let request = ::serde_json::json!({
            "method": "initialize_schema_context", "params": schema.request()
        });
        let output = self.run(&format!("{request}\n")).await?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut replies = super::responses(&output);
        assert_eq!(replies.len(), 1);
        Ok(::serde_json::from_value(
            replies.remove(0)["result"].clone(),
        )?)
    }

    pub fn executable(&self) -> &Path {
        self.0.executable()
    }
    pub fn user(&self) -> &str {
        self.0.user()
    }
    pub fn stop_owned_server(&self) -> ::testresult::TestResult {
        let pid = i32::try_from(self.registration()?["pid"].as_u64().unwrap())?;
        let running_executable = std::fs::read_link(format!("/proc/{pid}/exe"))?;
        assert_eq!(running_executable, self.executable());
        ::nix::sys::signal::kill(
            ::nix::unistd::Pid::from_raw(pid),
            ::nix::sys::signal::Signal::SIGKILL,
        )?;
        Ok(())
    }

    pub fn registration(&self) -> ::testresult::TestResult<::serde_json::Value> {
        self.0.registration()
    }

    pub async fn run(&self, input: &str) -> ::testresult::TestResult<std::process::Output> {
        Self::run_command(self.command(), input).await
    }

    pub async fn run_command(
        mut command: ::tokio::process::Command,
        input: &str,
    ) -> ::testresult::TestResult<std::process::Output> {
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).await?;
        drop(stdin);
        Ok(::tokio::time::timeout(Duration::from_secs(60), child.wait_with_output()).await??)
    }
}

/// A persistent API bridge owns the connection on which its handles are valid.
/// Starting the real bridge/server is costly and required for lifecycle assertions.
pub struct BridgeSession {
    child: ::tokio::process::Child,
    stdin: ::tokio::process::ChildStdin,
    stdout: ::tokio::io::BufReader<::tokio::process::ChildStdout>,
}
impl BridgeSession {
    pub fn start(server: &ApiServer) -> ::testresult::TestResult<Self> {
        let mut child = server.command().spawn()?;
        let stdin = child.stdin.take().unwrap();
        let stdout = ::tokio::io::BufReader::new(child.stdout.take().unwrap());
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }
    pub async fn rpc(
        &mut self,
        method: &str,
        params: &::serde_json::Value,
    ) -> ::testresult::TestResult<::serde_json::Value> {
        use ::tokio::io::AsyncBufReadExt;
        let request = ::serde_json::json!({"method":method, "params":params});
        self.stdin
            .write_all(format!("{request}\n").as_bytes())
            .await?;
        let mut line = String::new();
        ::tokio::time::timeout(Duration::from_secs(45), self.stdout.read_line(&mut line)).await??;
        Ok(::serde_json::from_str(&line)?)
    }
    pub async fn finish(self) -> ::testresult::TestResult {
        drop(self.stdin);
        let output = ::tokio::time::timeout(Duration::from_secs(45), self.child.wait_with_output())
            .await??;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}

/// Verify query handles on a persistent bridge, reusing the lifecycle test server.
pub async fn verify_query_analysis_session(server: &ApiServer) -> ::testresult::TestResult {
    let schema =
        super::schema::SchemaFixture::with_query_schema("CREATE TABLE entries(id int4 NOT NULL);")?;
    let mut bridge = BridgeSession::start(server)?;
    let initialized = bridge
        .rpc("initialize_schema_context", schema.request())
        .await?;
    let handle = &initialized["result"]["context_handle"];
    let malformed = bridge
        .rpc(
            "analyze_query",
            &::serde_json::json!({"context_handle":handle, "query":{"Text":"SELECT ("}}),
        )
        .await?;
    assert_eq!(
        malformed["result"]["queries"]["items"][0]["complete"],
        false
    );
    let response = bridge.rpc("analyze_query", &::serde_json::json!({"context_handle":handle, "query":{"Text":"SELECT ${id:int4} AS id; SELECT id FROM entries"}})).await?;
    let queries = &response["result"]["queries"];
    assert_eq!(queries["total_length"], 2, "{response}");
    let first = &queries["items"][0];
    assert_eq!(first["complete"], true, "{response}");
    assert!(first["execute_sql"].as_str().unwrap().contains("$1"));
    assert_eq!(first["parameters"]["items"][0]["parameter_index"], 1);
    assert_eq!(
        first["parameters"]["items"][0]["parameter_type"]["local_name"],
        "int4"
    );
    for collection in [queries, &first["parameters"], &first["result_columns"]] {
        let args = ::serde_json::json!({"collection_handle":collection["handle"], "start_offset":0, "count":null});
        assert_eq!(
            bridge.rpc("read_collection", &args).await?["result"],
            *collection
        );
        bridge
            .rpc(
                "release_collection",
                &::serde_json::json!({"collection_handle":collection["handle"]}),
            )
            .await?;
        assert_eq!(
            bridge.rpc("read_collection", &args).await?["error"]["data"],
            "InvalidCollectionHandle"
        );
    }
    bridge.finish().await?;
    Ok(())
}
