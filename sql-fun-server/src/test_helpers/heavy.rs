use crate::{Api, Args, ServerError, serve_client_with_api};
use std::sync::Arc;

use std::time::Duration;

use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream, tcp::OwnedReadHalf, tcp::OwnedWriteHalf},
    task::JoinHandle,
};

const WAIT: Duration = Duration::from_secs(5);

/// Starts a fresh test-owned TCP connection handler per fixture, without sharing.
/// Real sockets exercise authentication, buffered handoff, and transport teardown.
pub(crate) struct Connection {
    responses: Option<BufReader<OwnedReadHalf>>,
    requests: Option<OwnedWriteHalf>,
    handler: JoinHandle<Result<(), ServerError>>,
}

impl Connection {
    pub(crate) async fn start(request_length_limit: usize, auth_timeout: u64) -> Self {
        Self::start_with_api(
            request_length_limit,
            auth_timeout,
            &Arc::new(Api {
                schema_cache: std::sync::OnceLock::new(),
            }),
        )
        .await
    }

    /// Uses the selected test-owned cache host so parallel tests never share installations.
    pub(crate) async fn start_with_api(
        request_length_limit: usize,
        auth_timeout: u64,
        api: &Arc<Api>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            ::tokio::time::timeout(WAIT, TcpStream::connect(listener.local_addr().unwrap()))
                .await
                .unwrap()
                .unwrap();
        let (server, peer) = ::tokio::time::timeout(WAIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let args = Args {
            user: "session-test".to_owned(),
            request_length_limit,
            idle_timeout: 3,
            server_idle_timeout: 180,
            auth_timeout,
            token: "session-test-token".to_owned(),
        };
        let api = Arc::clone(api);
        let handler =
            ::tokio::spawn(async move { serve_client_with_api(&args, server, peer, &api).await });
        let (read, requests) = client.into_split();
        Self {
            responses: Some(BufReader::new(read)),
            requests: Some(requests),
            handler,
        }
    }

    pub(crate) async fn send(&mut self, frames: &str) {
        ::tokio::time::timeout(
            WAIT,
            self.requests.as_mut().unwrap().write_all(frames.as_bytes()),
        )
        .await
        .unwrap()
        .unwrap();
    }

    pub(crate) async fn rpc(
        &mut self,
        method: &str,
        params: &::serde_json::Value,
    ) -> ::serde_json::Value {
        let request = ::serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        self.send(&format!("{request}\n")).await;
        self.response().await
    }

    pub(crate) async fn authenticate(&mut self) {
        self.send(concat!(
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"sql_fun_token":"session-test-token"}}"#,
            "\n",
        )).await;
        assert_eq!(
            self.response().await,
            ::serde_json::json!({"jsonrpc":"2.0","id":0,"result":"OK"})
        );
    }

    pub(crate) async fn response(&mut self) -> ::serde_json::Value {
        let line = self.response_line().await.unwrap();
        assert!(line.ends_with('\n'), "response must be a JSON Lines frame");
        ::serde_json::from_str(&line).unwrap()
    }

    async fn response_line(&mut self) -> Result<String, std::io::Error> {
        let mut line = String::new();
        ::tokio::time::timeout(WAIT, self.responses.as_mut().unwrap().read_line(&mut line))
            .await
            .unwrap()?;
        Ok(line)
    }

    pub(crate) async fn end_requests(&mut self) {
        ::tokio::time::timeout(WAIT, self.requests.as_mut().unwrap().shutdown())
            .await
            .unwrap()
            .unwrap();
    }

    pub(crate) async fn assert_eof(&mut self) {
        assert_eq!(self.response_line().await.unwrap(), "");
    }

    pub(crate) async fn assert_no_response(&mut self) {
        // Rejection may close with FIN or reset when unread input remains.
        if let Ok(line) = self.response_line().await {
            assert!(
                line.is_empty(),
                "rejected request received a response: {line}"
            );
        }
    }

    pub(crate) async fn finish(&mut self) -> Result<(), ServerError> {
        ::tokio::time::timeout(WAIT, &mut self.handler)
            .await
            .unwrap()
            .unwrap()
    }

    /// Drop both client transport halves without the EOF/drain handshake.
    pub(crate) async fn disconnect(&mut self) {
        self.requests.take();
        self.responses.take();
        let _ = self.finish().await;
    }

    pub(crate) async fn cancel(&mut self) {
        self.handler.abort();
        let error = ::tokio::time::timeout(WAIT, &mut self.handler)
            .await
            .unwrap()
            .unwrap_err();
        assert!(error.is_cancelled());
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.handler.abort();
    }
}

/// Binds a fresh loopback listener per test, without schema setup or sharing.
/// Used to exercise the real accept loop when acceptance and expiry are ready.
pub(crate) async fn idle_listener() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").await.unwrap()
}
