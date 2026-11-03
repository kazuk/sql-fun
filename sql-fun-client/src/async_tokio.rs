pub(crate) mod streaming;

#[cfg(test)]
mod test_helpers;

use crate::{CompleteResponse, CompletionContext, TypedResponseError};

use self::streaming::Streaming;

use crate::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

use ::tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

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
    #[track_caller]
    fn raise_connection_closed<T>() -> Result<T, Self> {
        Err(Self::Io(std::io::Error::other("connection closed")))
    }

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
            return streaming.peer_addr();
        }
        let Some(reader) = self.reader.as_ref() else {
            return Err(std::io::Error::other("connection closed"));
        };
        reader.get_ref().peer_addr()
    }

    /// Submit a request with an infallible asynchronous response callback.
    ///
    /// Acceptance does not wait for a response. At most 8 accepted user requests,
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
        streaming.submit_request(method, params, callback).await?;
        Ok(())
    }

    /// Submit a typed response for full collection materialization and release.
    ///
    /// The callback runs exactly once after acceptance, with the completed result
    /// or a decoding, collection, server, or connection error. Selecting this API
    /// authorizes reading all pages into memory and releasing retained handles.
    /// Use `invoke_method` for explicit manual paging instead. Dropping the client
    /// or cancelling `finish` aborts callbacks and cannot guarantee release.
    ///
    /// ```no_run
    /// # async fn example(mut client: sql_fun_client::TcpJsonRpcClientAsync,
    /// # params: serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    /// client.invoke_typed("initialize_schema_context", &params,
    ///     |result: Result<sql_fun_client::payloads::SchemaContext, _>| async move {
    ///         if let Ok(context) = result {
    ///             assert!(context.problems().is_completed());
    ///         }
    ///         Ok(())
    ///     }).await?;
    /// client.finish().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns submission failures before acceptance. No callback runs in that case.
    pub async fn invoke_typed<R, F, Fut>(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        callback: F,
    ) -> Result<(), TcpJsonRpcClientAsyncError>
    where
        R: CompleteResponse + ::serde::de::DeserializeOwned + Send + 'static,
        F: FnOnce(Result<R, TypedResponseError>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), std::convert::Infallible>> + Send + 'static,
    {
        self.start_streaming()?;
        let sender = self.streaming_connection()?.sender();
        let completion_sender = sender.clone();
        let typed = sender.begin_typed();
        sender
            .submit_user(
                method,
                params,
                Box::new(move |response| {
                    Box::pin(async move {
                        let _typed = typed;
                        let mut context = CompletionContext::new(completion_sender);
                        let result = async {
                            let response = response
                                .map_err(|message| TypedResponseError::message(&message))?;
                            let value = response.into_result()?;
                            let mut value: R = ::serde_json::from_value(value)?;
                            value.register_collections(&mut context);
                            value.complete(&mut context).await?;
                            Ok(value)
                        }
                        .await;
                        let result = context.finish(result).await;
                        callback(result).await
                    })
                }),
            )
            .await
    }

    /// Wait for a terminal connection failure independently of submission.
    ///
    /// # Errors
    /// Returns the observed transport/protocol failure or connection closure.
    pub async fn wait_for_failure(&mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.start_streaming()?;
        let streaming = self.streaming_connection()?;
        streaming.wait_for_failure().await
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
        let Some(streaming) = self.streaming.take() else {
            return TcpJsonRpcClientAsyncError::raise_connection_closed();
        };
        streaming.finish().await
    }

    fn streaming_connection(&self) -> Result<&Streaming, TcpJsonRpcClientAsyncError> {
        let Some(streaming) = self.streaming.as_ref() else {
            return TcpJsonRpcClientAsyncError::raise_connection_closed();
        };
        Ok(streaming)
    }

    fn start_streaming(&mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        if self.streaming.is_none() {
            let Some(reader) = self.reader.take() else {
                return TcpJsonRpcClientAsyncError::raise_connection_closed();
            };
            self.streaming = Some(Streaming::start(reader, self.id)?);
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
            return TcpJsonRpcClientAsyncError::raise_connection_closed();
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
        let Some(response) = read_response(&mut reader, Some(limit)).await? else {
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
    use crate::async_tokio::test_helpers;
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
    async fn delivers_response_larger_than_two_mebibytes() -> ::testresult::TestResult {
        let payload = "x".repeat(2 * 1024 * 1024);
        let response = ::serde_json::json!({"jsonrpc":"2.0", "id":0, "result":payload});
        let mut frame = ::serde_json::to_vec(&response)?;
        frame.push(b'\n');
        let mut client = test_helpers::response_peer(&frame).await?;
        let (sent, received) = ::tokio::sync::oneshot::channel();
        client
            .invoke_method(
                "large_response",
                &::serde_json::json!({"sql_fun_token":"secret"}),
                move |response| async move {
                    let _ = sent.send(response.into_result());
                    Ok(())
                },
            )
            .await?;
        let result = ::tokio::time::timeout(Duration::from_secs(5), received).await???;
        assert_eq!(result, payload);
        ::tokio::time::timeout(Duration::from_secs(5), client.finish()).await??;
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
        let mut client = test_helpers::draining_peer(8, false).await?;
        for _ in 0..8 {
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
    use crate::async_tokio::test_helpers;
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

async fn read_response<R: ::tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: Option<usize>,
) -> Result<Option<JsonRpcResponse>, TcpJsonRpcClientAsyncError> {
    let mut frame = Vec::new();
    let count = reader
        .take(limit.map_or(u64::MAX, |limit| limit as u64 + 1))
        .read_until(b'\n', &mut frame)
        .await?;
    if count == 0 {
        return Ok(None);
    }
    if limit.is_some_and(|limit| count > limit) || frame.last() != Some(&b'\n') {
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
