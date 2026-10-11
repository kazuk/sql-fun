use crate::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

use ::tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::TcpStream;

/// Errors from a sequential async JSON-RPC exchange.
#[derive(Debug, thiserror::Error)]
pub enum TcpJsonRpcClientAsyncError {
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
    pub(crate) fn initialization_diagnostic(&self) -> String {
        match self {
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

/// Async newline-delimited JSON-RPC client owning its socket directly.
///
/// Drop closes the connection. Transport failures or cancelled calls close it, preventing
/// a partial exchange from being mistaken for the next response.
#[derive(Debug)]
pub struct TcpJsonRpcClientAsync {
    reader: Option<BufReader<TcpStream>>,
    id: u64,
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
        })
    }

    /// Return the connected server address.
    ///
    /// # Errors
    /// Returns an error if the socket is closed or its address cannot be read.
    pub fn peer_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.reader
            .as_ref()
            .ok_or_else(|| std::io::Error::other("connection closed"))?
            .get_ref()
            .peer_addr()
    }

    /// Invoke a method with JSON object parameters and a bounded response frame.
    ///
    /// # Errors
    /// Returns transport, framing, serialization, or server API errors.
    /// Calls after establishment have no implicit time deadline; callers may wrap
    /// them in `tokio::time::timeout`. Cancellation closes the socket.
    pub async fn invoke_method(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, TcpJsonRpcClientAsyncError> {
        self.invoke_bounded(method, params, 2 * 1024 * 1024).await
    }

    pub(crate) async fn invoke_bounded(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        limit: usize,
    ) -> Result<serde_json::Value, TcpJsonRpcClientAsyncError> {
        let mut reader = self
            .reader
            .take()
            .ok_or_else(|| std::io::Error::other("connection closed"))?;
        let id = self.id;
        self.id += 1;
        let request = JsonRpcRequest::build_request(id, method, params);
        let mut json = ::serde_json::to_vec(&request)?;
        json.push(b'\n');
        reader.get_mut().write_all(&json).await?;
        let mut frame = Vec::new();
        // Take bounds allocation even when the peer never sends a newline. The
        // original buffered reader is retained, including bytes past this frame.
        let count = (&mut reader)
            .take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
            .read_until(b'\n', &mut frame)
            .await?;
        if count == 0 || frame.last() != Some(&b'\n') || count > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response EOF, incomplete frame, or frame exceeds limit",
            )
            .into());
        }
        // Validate the exclusive result/error envelope before deserialization.
        let value: ::serde_json::Value = ::serde_json::from_slice(&frame)?;
        if value.get("result").is_some() == value.get("error").is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response must contain exactly one of result or error",
            )
            .into());
        }
        let response: JsonRpcResponse = ::serde_json::from_value(value)?;
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
mod test_helpers;

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
                .invoke_method(
                    "initialize",
                    &::serde_json::json!({"sql_fun_token":"secret"})
                )
                .await
                .is_err()
        );
        assert_eq!(
            client
                .invoke_method("next", &::serde_json::json!({}))
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
            client.invoke_method(
                "initialize",
                &::serde_json::json!({"sql_fun_token": "secret"}),
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
                .invoke_method(
                    "initialize",
                    &::serde_json::json!({"sql_fun_token":"secret"})
                )
                .await?,
            "OK"
        );
        assert_eq!(
            client
                .invoke_method("next", &::serde_json::json!({}))
                .await?,
            42
        );
        Ok(())
    }
}
