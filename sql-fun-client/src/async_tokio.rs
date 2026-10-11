#[cfg(test)]
mod test_helpers;

use crate::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

use ::tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::TcpStream;

/// Errors from submission, transport, or graceful completion.
#[derive(Debug, thiserror::Error)]
pub enum TcpJsonRpcClientAsyncError {
    /// Connection worker failure.
    #[error("connection failed: {0}")]
    Connection(String),
    /// Serialization or invalid response JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Socket or framing failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An error response from the server.
    #[error(transparent)]
    JsonRpc(#[from] JsonRpcError),
}

impl TcpJsonRpcClientAsyncError {
    fn connection(message: &str) -> Self {
        Self::Connection(message.to_owned())
    }
    pub(crate) fn initialization_diagnostic(&self) -> String {
        match self {
            Self::Connection(_) => String::from("initialize: connection failure"),
            Self::Json(_) => {
                String::from("initialize: malformed JSON or invalid protocol version/envelope")
            }
            Self::Io(error) => format!(
                "initialize: framing, EOF/closure, or socket failure ({})",
                error.kind()
            ),
            Self::JsonRpc(_) => String::from("initialize: server returned an error response"),
        }
    }
}

/// Async newline-delimited JSON-RPC client with bounded concurrent submissions.
///
/// Drop closes its socket and tasks. Use [`Self::finish`] to drain accepted work.
#[derive(Debug)]
pub struct TcpJsonRpcClientAsync {
    reader: Option<BufReader<TcpStream>>,
    id: u64,
    streaming: Option<Streaming>,
}

impl TcpJsonRpcClientAsync {
    /// Wrap an already connected stream. This low-level operation does not authenticate.
    ///
    /// # Errors
    /// This constructor currently has no failure conditions.
    pub fn try_from(stream: TcpStream) -> Result<Self, std::io::Error> {
        Ok(Self {
            reader: Some(BufReader::new(stream)),
            id: 0,
            streaming: None,
        })
    }

    /// Return the connected server address.
    ///
    /// # Errors
    /// Returns an error if the socket is closed or its address cannot be read.
    pub fn peer_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        if let Some(streaming) = &self.streaming {
            return if streaming.status.borrow().is_none() {
                Ok(streaming.address)
            } else {
                Err(std::io::Error::other("connection closed"))
            };
        }
        let Some(reader) = self.reader.as_ref() else {
            return Err(std::io::Error::other("connection closed"));
        };
        reader.get_ref().peer_addr()
    }

    /// Submit a request with an infallible asynchronous response callback.
    ///
    /// Acceptance does not wait for a response. At most 32 accepted requests,
    /// including running callbacks, can remain unfinished. Cancellation before
    /// acceptance leaves no pending request. Callbacks must not wait for further
    /// submissions to this same saturated client.
    ///
    /// # Errors
    /// Returns serialization, exhausted ID, or connection errors.
    pub async fn invoke_method<F, Fut>(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        callback: F,
    ) -> Result<(), TcpJsonRpcClientAsyncError>
    where
        F: FnOnce(JsonRpcResponse) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), std::convert::Infallible>> + Send + 'static,
    {
        self.start_streaming()?;
        let streaming = self.streaming_connection()?;
        let permit = streaming
            .capacity
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| streaming.error())?;
        let Some(next) = self.id.checked_add(1) else {
            return Err(TcpJsonRpcClientAsyncError::connection(
                "request IDs exhausted",
            ));
        };
        let request = JsonRpcRequest::build_request(self.id, method, params);
        let mut frame = ::serde_json::to_vec(&request)?;
        frame.push(b'\n');
        let Some(sender) = streaming.sender.as_ref() else {
            return Err(streaming.error());
        };
        sender
            .send(Submission {
                id: self.id,
                method: method.to_owned(),
                frame,
                callback: Box::new(move |response| Box::pin(callback(response))),
                permit,
            })
            .await
            .map_err(|_| streaming.error())?;
        self.id = next;
        Ok(())
    }

    /// Wait for a terminal connection failure independently of submission.
    ///
    /// # Errors
    /// Returns the observed transport/protocol failure or connection closure.
    pub async fn wait_for_failure(&mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.start_streaming()?;
        let streaming = self.streaming_connection()?;
        let mut status = streaming.status.clone();
        while status.borrow().is_none() {
            if status.changed().await.is_err() {
                break;
            }
        }
        Err(streaming.error())
    }

    /// Flush accepted requests, half-close writes, drain responses and callbacks.
    ///
    /// No implicit timeout applies. Cancelling completion closes the connection.
    /// The shared server remains running.
    ///
    /// # Errors
    /// Returns transport/protocol failures, including missing responses at EOF.
    pub async fn finish(mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.start_streaming()?;
        let Some(mut streaming) = self.streaming.take() else {
            return Err(std::io::Error::other("connection closed").into());
        };
        streaming.sender.take();
        (&mut streaming.worker)
            .await
            .map_err(|error| TcpJsonRpcClientAsyncError::connection(&error.to_string()))?
            .map_err(|error| TcpJsonRpcClientAsyncError::connection(&error))
    }

    fn streaming_connection(&self) -> Result<&Streaming, TcpJsonRpcClientAsyncError> {
        let Some(streaming) = self.streaming.as_ref() else {
            return Err(std::io::Error::other("connection closed").into());
        };
        Ok(streaming)
    }

    fn start_streaming(&mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        if self.streaming.is_none() {
            let Some(reader) = self.reader.take() else {
                return Err(std::io::Error::other("connection closed").into());
            };
            self.streaming = Some(Streaming::start(reader)?);
        }
        Ok(())
    }

    pub(crate) async fn invoke_bounded(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        limit: usize,
    ) -> Result<serde_json::Value, TcpJsonRpcClientAsyncError> {
        let Some(mut reader) = self.reader.take() else {
            return Err(std::io::Error::other("connection closed").into());
        };
        let id = self.id;
        let Some(next) = self.id.checked_add(1) else {
            return Err(TcpJsonRpcClientAsyncError::connection(
                "request IDs exhausted",
            ));
        };
        self.id = next;
        let request = JsonRpcRequest::build_request(id, method, params);
        let mut json = ::serde_json::to_vec(&request)?;
        json.push(b'\n');
        reader.get_mut().write_all(&json).await?;
        ::tracing::info!(request_id = id, method, "request written");
        let Some(response) = read_response(&mut reader, limit).await? else {
            return Err(
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "response EOF").into(),
            );
        };
        response.trace_received();
        if response.id != id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response ID does not match request",
            )
            .into());
        }
        self.reader = Some(reader);
        Ok(response.body.into_result()?)
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::test_helpers;
    use std::time::Duration;

    #[rstest::rstest]
    #[case::unknown(concat!(r#"{"jsonrpc":"2.0","id":9,"result":null}"#, "\n"))]
    #[case::duplicate(concat!(r#"{"jsonrpc":"2.0","id":0,"result":null}"#, "\n", r#"{"jsonrpc":"2.0","id":0,"result":null}"#, "\n"))]
    #[case::version(concat!(r#"{"jsonrpc":"1.0","id":0,"result":null}"#, "\n"))]
    #[case::both(concat!(r#"{"jsonrpc":"2.0","id":0,"result":null,"error":{"code":1,"message":"bad"}}"#, "\n"))]
    #[case::malformed("not JSON\n")]
    #[tokio::test]
    async fn streaming_protocol_failure_is_observable_without_submission(
        #[case] frame: &str,
    ) -> ::testresult::TestResult {
        let mut client = test_helpers::response_peer(frame.as_bytes()).await?;
        client
            .invoke_method(
                "probe",
                &::serde_json::json!({"sql_fun_token":"secret"}),
                |_| async { Ok(()) },
            )
            .await?;
        assert!(
            ::tokio::time::timeout(Duration::from_secs(5), client.wait_for_failure())
                .await?
                .is_err()
        );
        assert!(client.peer_addr().is_err());
        assert!(
            client
                .invoke_method("later", &::serde_json::json!({}), |_| async { Ok(()) })
                .await
                .is_err()
        );
        assert!(client.finish().await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn slow_callback_does_not_block_delivery_and_finish_waits() -> ::testresult::TestResult {
        ::tokio::time::timeout(Duration::from_secs(5), async {
            let mut client = test_helpers::eager_peer().await?;
            let (release, wait) = ::tokio::sync::oneshot::channel();
            let (started, running) = ::tokio::sync::oneshot::channel();
            client
                .invoke_method("first", &::serde_json::json!({}), move |_| async move {
                    let _ = started.send(());
                    let _ = wait.await;
                    Ok(())
                })
                .await?;
            let (delivered, completed) = ::tokio::sync::oneshot::channel();
            client
                .invoke_method("second", &::serde_json::json!({}), move |_| async move {
                    let _ = delivered.send(());
                    Ok(())
                })
                .await?;
            running.await?;
            completed.await?;
            let completion = client.finish();
            ::tokio::pin!(completion);
            assert!(
                ::tokio::time::timeout(Duration::from_millis(30), &mut completion)
                    .await
                    .is_err()
            );
            let _ = release.send(());
            completion.await?;
            ::testresult::TestResult::Ok(())
        })
        .await??;
        Ok(())
    }

    #[tokio::test]
    async fn id_exhaustion_rejects_submission() -> ::testresult::TestResult {
        let mut client = test_helpers::draining_peer(0, false).await?;
        client.id = u64::MAX;
        let error = client
            .invoke_method("exhausted", &::serde_json::json!({}), |_| async { Ok(()) })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("IDs exhausted"));
        ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await??;
        Ok(())
    }

    #[tokio::test]
    async fn tracing_reports_transport_metadata_without_payloads() -> ::testresult::TestResult {
        let logs = test_helpers::LogCapture::default();
        let subscriber = ::tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(::tracing::Level::INFO)
            .with_writer(logs.clone())
            .finish();
        ::tracing::subscriber::set_global_default(subscriber)?;
        let mut client = test_helpers::response_peer(
            br#"{"jsonrpc":"2.0","id":0,"error":{"code":-32603,"message":"private-response"}}
"#,
        )
        .await?;
        client
            .invoke_method(
                "tracing_probe",
                &::serde_json::json!({"sql_fun_token":"secret"}),
                |_| async { Ok(()) },
            )
            .await?;
        ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await??;
        let output = logs.text();
        let request = output
            .lines()
            .find(|line| line.contains("tracing_probe"))
            .expect("request event");
        assert!(request.contains("INFO") && request.contains("request_id=0"));
        let response = output
            .lines()
            .find(|line| line.contains("error_code=-32603"))
            .expect("response event");
        assert!(response.contains("INFO") && response.contains("request_id=0"));
        assert!(!response.contains("method="));
        assert!(
            !output.contains("secret")
                && !output.contains("private-response")
                && !output.contains("sql_fun_token")
        );
        Ok(())
    }

    #[tokio::test]
    async fn submissions_and_reverse_responses_drain_after_half_close() -> ::testresult::TestResult
    {
        ::tokio::time::timeout(Duration::from_secs(5), async {
            let mut client = test_helpers::draining_peer(3, true).await?;
            let (sent, mut received) = ::tokio::sync::mpsc::unbounded_channel();
            for number in 0..3 {
                let sent = sent.clone();
                client
                    .invoke_method(
                        "echo",
                        &::serde_json::json!({"number":number}),
                        move |response| async move {
                            assert_eq!(response.id(), number);
                            assert_eq!(
                                response.into_result().unwrap(),
                                ::serde_json::json!({"number":number})
                            );
                            let _ = sent.send(number);
                            Ok(())
                        },
                    )
                    .await?;
            }
            client.finish().await?;
            let mut delivered = Vec::new();
            for _ in 0..3 {
                delivered.push(received.recv().await.unwrap());
            }
            delivered.sort_unstable();
            assert_eq!(delivered, vec![0, 1, 2]);
            ::testresult::TestResult::Ok(())
        })
        .await??;
        Ok(())
    }

    #[tokio::test]
    async fn missing_response_is_reported_by_completion() -> ::testresult::TestResult {
        let mut client = test_helpers::draining_peer(0, false).await?;
        client
            .invoke_method("echo", &::serde_json::json!({}), |_| async { Ok(()) })
            .await?;
        let result = ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await?;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing responses")
        );
        Ok(())
    }

    #[tokio::test]
    async fn bounded_admission_cancellation_does_not_register_request() -> ::testresult::TestResult
    {
        let mut client = test_helpers::draining_peer(32, false).await?;
        for _ in 0..32 {
            client
                .invoke_method("echo", &::serde_json::json!([]), |_| async { Ok(()) })
                .await?;
        }
        assert!(
            ::tokio::time::timeout(
                Duration::from_millis(30),
                client.invoke_method("cancelled", &::serde_json::json!([]), |_| async {
                    panic!("cancelled request delivered")
                })
            )
            .await
            .is_err()
        );
        ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers;
    use std::time::Duration;

    #[rstest::rstest]
    #[case::version(
        br#"{"jsonrpc":"1.0","id":0,"result":"OK"}
"#
    )]
    #[case::id(
        br#"{"jsonrpc":"2.0","id":9,"result":"OK"}
"#
    )]
    #[case::missing_result(
        br#"{"jsonrpc":"2.0","id":0}
"#
    )]
    #[case::both(
        br#"{"jsonrpc":"2.0","id":0,"result":"OK","error":{"code":-32603,"message":"bad"}}
"#
    )]
    #[case::malformed(b"not JSON\n")]
    #[case::oversized(b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n")]
    #[tokio::test]
    async fn invalid_responses_close_connection(#[case] frame: &[u8]) -> ::testresult::TestResult {
        let mut client = test_helpers::response_peer(frame).await?;
        let result = client
            .invoke_bounded(
                "initialize",
                &::serde_json::json!({"sql_fun_token": "secret"}),
                128,
            )
            .await;
        assert!(result.is_err());
        assert!(client.peer_addr().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn valid_error_response_keeps_connection_usable() -> ::testresult::TestResult {
        let mut client = test_helpers::response_peer(
            br#"{"jsonrpc":"2.0","id":0,"error":{"code":-32603,"message":"bad"}}
{"jsonrpc":"2.0","id":1,"result":"OK"}
"#,
        )
        .await?;
        assert!(
            client
                .invoke_bounded(
                    "initialize",
                    &::serde_json::json!({"sql_fun_token":"secret"}),
                    8192
                )
                .await
                .is_err()
        );
        assert_eq!(
            client
                .invoke_bounded("next", &::serde_json::json!({}), 8192)
                .await?,
            "OK"
        );
        Ok(())
    }

    #[tokio::test]
    async fn cancellation_closes_stalled_exchange() -> ::testresult::TestResult {
        let mut client = test_helpers::response_peer(b"{\"jsonrpc\":").await?;
        let result = ::tokio::time::timeout(
            Duration::from_millis(30),
            client.invoke_bounded(
                "initialize",
                &::serde_json::json!({"sql_fun_token": "secret"}),
                8192,
            ),
        )
        .await;
        assert!(result.is_err());
        assert!(client.peer_addr().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn retains_buffered_bytes_between_exchanges() -> ::testresult::TestResult {
        let mut client = test_helpers::response_peer(
            br#"{"jsonrpc":"2.0","id":0,"result":"OK"}
{"jsonrpc":"2.0","id":1,"result":42}
"#,
        )
        .await?;
        assert_eq!(
            client
                .invoke_bounded(
                    "initialize",
                    &::serde_json::json!({"sql_fun_token":"secret"}),
                    8192
                )
                .await?,
            "OK"
        );
        assert_eq!(
            client
                .invoke_bounded("next", &::serde_json::json!({}), 8192)
                .await?,
            42
        );
        Ok(())
    }
}

type CallbackFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(), std::convert::Infallible>> + Send>,
>;
type Callback = Box<dyn FnOnce(JsonRpcResponse) -> CallbackFuture + Send>;

struct Submission {
    id: u64,
    method: String,
    frame: Vec<u8>,
    callback: Callback,
    permit: ::tokio::sync::OwnedSemaphorePermit,
}

struct Streaming {
    sender: Option<::tokio::sync::mpsc::Sender<Submission>>,
    capacity: std::sync::Arc<::tokio::sync::Semaphore>,
    status: ::tokio::sync::watch::Receiver<Option<String>>,
    worker: ::tokio::task::JoinHandle<Result<(), String>>,
    address: std::net::SocketAddr,
}

impl std::fmt::Debug for Streaming {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Streaming")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Drop for Streaming {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl Streaming {
    fn error(&self) -> TcpJsonRpcClientAsyncError {
        TcpJsonRpcClientAsyncError::connection(
            &self
                .status
                .borrow()
                .clone()
                .unwrap_or_else(|| String::from("connection closed")),
        )
    }

    fn start(reader: BufReader<TcpStream>) -> std::io::Result<Self> {
        let address = reader.get_ref().peer_addr()?;
        let (sender, receiver) = ::tokio::sync::mpsc::channel(32);
        let capacity = std::sync::Arc::new(::tokio::sync::Semaphore::new(32));
        let (status_sender, status) = ::tokio::sync::watch::channel(None);
        let worker_capacity = capacity.clone();
        let worker = ::tokio::spawn(async move {
            let result = Self::run(reader, receiver, &status_sender, &worker_capacity).await;
            worker_capacity.close();
            status_sender.send_replace(Some(match &result {
                Ok(()) => String::from("connection closed"),
                Err(error) => error.clone(),
            }));
            result
        });
        Ok(Self {
            sender: Some(sender),
            capacity,
            status,
            worker,
            address,
        })
    }

    async fn write_requests<W: ::tokio::io::AsyncWrite + Unpin>(
        mut write: W,
        mut requests: ::tokio::sync::mpsc::UnboundedReceiver<(u64, String, Vec<u8>)>,
    ) -> Result<(), String> {
        let result: Result<(), std::io::Error> = async {
            while let Some((id, method, frame)) = requests.recv().await {
                write.write_all(&frame).await?;
                ::tracing::info!(request_id = id, method, "request written");
            }
            write.flush().await?;
            write.shutdown().await
        }
        .await;
        result.map_err(|error| error.to_string())
    }

    async fn run(
        reader: BufReader<TcpStream>,
        mut submissions: ::tokio::sync::mpsc::Receiver<Submission>,
        status: &::tokio::sync::watch::Sender<Option<String>>,
        capacity: &::tokio::sync::Semaphore,
    ) -> Result<(), String> {
        let (read, write) = ::tokio::io::split(reader);
        let (outgoing, writes) = ::tokio::sync::mpsc::unbounded_channel::<(u64, String, Vec<u8>)>();
        let (events, mut responses) = ::tokio::sync::mpsc::channel(32);
        let mut transport = ::tokio::task::JoinSet::new();
        transport.spawn(Self::write_requests(write, writes));
        transport.spawn(async move {
            let mut read = BufReader::new(read);
            loop {
                match read_response(&mut read, 2 * 1024 * 1024).await {
                    Ok(response) => {
                        let eof = response.is_none();
                        if events.send(Ok(response)).await.is_err() || eof {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = events.send(Err(error.to_string())).await;
                        break;
                    }
                }
            }
            Ok(())
        });
        let mut outgoing = Some(outgoing);
        let mut pending = std::collections::HashMap::new();
        let mut callbacks = ::tokio::task::JoinSet::new();
        let result = loop {
            ::tokio::select! {
                submission = submissions.recv(), if outgoing.is_some() => {
                    if let Some(submission) = submission {
                        let Submission { id, method, frame, callback, permit } = submission;
                        pending.insert(id, (callback, permit));
                        if outgoing.as_ref().expect("writer open").send((id, method, frame)).is_err() {
                            break Err(String::from("request writer closed"));
                        }
                    } else { outgoing.take(); }
                }
                event = responses.recv() => {
                    match event {
                        Some(Ok(Some(response))) => {
                            response.trace_received();
                            let Some((callback, permit)) = pending.remove(&response.id) else {
                                break Err(format!("unexpected response ID {}", response.id));
                            };
                            callbacks.spawn(async move {
                                let Ok(()) = callback(response).await;
                                // Keep admission bounded until the completed task is reaped.
                                permit
                            });
                        }
                        Some(Ok(None)) => {
                            if !pending.is_empty() || !submissions.is_empty() {
                                break Err(String::from("EOF with missing responses"));
                            }
                            if outgoing.is_some() { break Err(String::from("unexpected connection EOF")); }
                            break Ok(());
                        }
                        Some(Err(error)) => break Err(error),
                        None => break Err(String::from("response reader closed")),
                    }
                }
                _ = callbacks.join_next(), if !callbacks.is_empty() => {}
                completed = transport.join_next(), if !transport.is_empty() => {
                    match completed {
                        Some(Ok(Err(error))) => break Err(error),
                        Some(Err(error)) => break Err(error.to_string()),
                        _ => {},
                    }
                }
            }
        };
        if let Err(error) = &result {
            status.send_replace(Some(error.clone()));
            capacity.close();
            submissions.close();
        }
        let mut result = result;
        if result.is_err() {
            transport.abort_all();
        }
        while let Some(completed) = transport.join_next().await {
            match completed {
                Ok(Err(error)) if result.is_ok() => result = Err(error),
                Err(error) if result.is_ok() => result = Err(error.to_string()),
                _ => {}
            }
        }
        // Track completion only: application callback failures are outside this contract.
        while callbacks.join_next().await.is_some() {}
        result
    }
}

async fn read_response<R: ::tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<JsonRpcResponse>, TcpJsonRpcClientAsyncError> {
    let mut frame = Vec::new();
    let count = reader
        .take(limit as u64 + 1)
        .read_until(b'\n', &mut frame)
        .await?;
    if count == 0 {
        return Ok(None);
    }
    if count > limit || frame.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "incomplete or oversized response frame",
        )
        .into());
    }
    let value: ::serde_json::Value = ::serde_json::from_slice(&frame)?;
    if value.get("result").is_some() == value.get("error").is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "response must contain exactly one of result or error",
        )
        .into());
    }
    Ok(Some(::serde_json::from_value(value)?))
}
