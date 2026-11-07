use std::{path::Path, time::Duration};

use ::serde_json::Value;
use ::sql_fun_client::ConnectionOptions;
use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    task::JoinHandle,
};

/// An isolated authenticated wire peer, without a server build or catalog parsing.
/// Replies after initialization contain the supplied result; drop aborts the peer.
pub struct ResponsePeer {
    directory: ::tempfile::TempDir,
    task: JoinHandle<()>,
}

impl ResponsePeer {
    pub async fn new(result: &Value) -> ::testresult::TestResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let directory = ::tempfile::Builder::new()
            .prefix("sql-fun-receive-")
            .tempdir_in("/tmp")?;
        std::fs::write(
            directory.path().join("sql-fun-server.pid"),
            ::serde_json::to_vec(&::serde_json::json!({
                "pid":std::process::id(), "port":listener.local_addr()?.port(), "token":"test-token"
            }))?,
        )?;
        let result = result.clone();
        let task = ::tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            loop {
                let mut request = String::new();
                if reader.read_line(&mut request).await.unwrap_or(0) == 0 {
                    break;
                }
                let request: Value = ::serde_json::from_str(&request).unwrap();
                let result = if request["method"] == "initialize" {
                    assert_eq!(request["params"]["sql_fun_token"], "test-token");
                    &::serde_json::json!("OK")
                } else {
                    &result
                };
                let mut frame = ::serde_json::to_vec(&::serde_json::json!({
                    "jsonrpc":"2.0", "id":request["id"], "result":result
                }))
                .unwrap();
                frame.push(b'\n');
                if reader.get_mut().write_all(&frame).await.is_err() {
                    break; // Oversized responses deliberately close the client socket.
                }
            }
        });
        Ok(Self { directory, task })
    }

    pub fn user(&self) -> &str {
        self.directory.path().file_name().unwrap().to_str().unwrap()
    }

    pub fn executable() -> &'static Path {
        Path::new("/nonexistent-sql-fun-server")
    }

    #[allow(
        dead_code,
        reason = "CLI fixtures select the setting through command inputs"
    )]
    pub fn options(&self, max_response_bytes: Option<u32>) -> ConnectionOptions {
        ConnectionOptions::builder()
            .server_executable(Self::executable())
            .user(self.user())
            .timeout(Duration::from_millis(200))
            .maybe_max_response_bytes(max_response_bytes)
            .build()
    }
}

impl Drop for ResponsePeer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A protocol-compatible analysis result with an explicitly sized executable SQL string.
pub fn analysis_result(sql_bytes: usize) -> Value {
    let empty = ::serde_json::json!({"handle":"empty", "total_length":0, "offset":0, "require_release":false, "items":[]});
    let item = ::serde_json::json!({
        "query_span":null, "query_kind":"Select", "execute_sql":format!("SELECT '{}'", "x".repeat(sql_bytes)),
        "complete":true, "probrems":empty, "parameters":empty,
        "result_columns":empty, "column_access":empty
    });
    ::serde_json::json!({"queries":{"handle":"queries", "total_length":1, "offset":0, "require_release":false, "items":[item]}})
}
