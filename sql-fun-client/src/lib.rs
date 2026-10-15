//! Clients for the per-user sql-fun server.

#![deny(missing_docs)]

mod async_tokio;
mod completion;
mod connection;
/// api payload
pub mod payloads;

pub use self::async_tokio::{TcpJsonRpcClientAsync, TcpJsonRpcClientAsyncError};
pub use self::completion::{
    CompleteResponse, CompletionContext, CompletionFuture, TypedResponseError,
};
pub use self::connection::{ConnectionError, ConnectionOptions, ConnectionOptionsBuilder, connect};

use crate::payloads::CollectionHandle;

use std::{
    io::{BufReader, Write},
    net::TcpStream,
};

/// client side collection representation
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(from = "StreamingCollection<T>")]
pub enum Collection<T> {
    /// raw server response
    Stream(StreamingCollection<T>),
    /// Fully materialized items whose required release succeeded.
    Completed(Vec<T>),
}

impl<T> Collection<T> {
    /// create [`Self::Stream`] variant
    #[must_use]
    pub fn new(
        handle: &CollectionHandle,
        total_length: usize,
        offset: usize,
        require_release: bool,
        items: Vec<T>,
    ) -> Self {
        Self::Stream(StreamingCollection::new(
            handle,
            total_length,
            offset,
            require_release,
            items,
        ))
    }

    /// Construct an already completed collection without retained server state.
    #[must_use]
    pub const fn completed(items: Vec<T>) -> Self {
        Self::Completed(items)
    }

    /// Whether all items and required releases have completed.
    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }

    /// handle for collection
    #[must_use]
    pub const fn handle(&self) -> Option<&CollectionHandle> {
        match self {
            Self::Stream(s) => Some(&s.handle),
            Self::Completed(_) => None,
        }
    }

    /// collection items
    #[must_use]
    pub fn items(&self) -> &[T] {
        match self {
            Self::Stream(s) => &s.items,
            Self::Completed(items) => items,
        }
    }
}

#[cfg(test)]
mod collection_tests {
    #[test]
    fn official_wire_page_starts_in_stream_state() -> ::testresult::TestResult {
        let handle = ::sql_fun_server_api::payloads::CollectionHandle::from("wire".to_owned());
        let wire = ::sql_fun_server_api::Collection::new(&handle, 3, 1, true, vec![7_u32]);
        let client: super::Collection<u32> =
            ::serde_json::from_value(::serde_json::to_value(wire)?)?;
        assert!(!client.is_completed());
        assert_eq!(client.items(), &[7]);
        assert_eq!(
            ::serde_json::to_value(client.handle())?,
            ::serde_json::json!("wire")
        );
        Ok(())
    }
}

impl<T> From<StreamingCollection<T>> for Collection<T> {
    fn from(value: StreamingCollection<T>) -> Self {
        Self::Stream(value)
    }
}

/// raw server response
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct StreamingCollection<T> {
    /// handle for collecttion
    handle: CollectionHandle,

    /// length of collection
    total_length: usize,

    /// offset of items
    offset: usize,

    /// When true, client must be call `SqlFunServerApi::release_collection`
    require_release: bool,

    /// collection items
    items: Vec<T>,
}

impl<T> StreamingCollection<T> {
    /// create instance
    #[must_use]
    pub fn new(
        handle: &CollectionHandle,
        total_length: usize,
        offset: usize,
        require_release: bool,
        items: Vec<T>,
    ) -> Self {
        Self {
            handle: handle.clone(),
            total_length,
            offset,
            require_release,
            items,
        }
    }
}

/// JSON RPC Client errors
#[derive(Debug, thiserror::Error)]
pub enum JsonRpcClientError {
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON RPC error
    #[error(transparent)]
    JsonRpc(#[from] JsonRpcError),

    /// serde error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    /// unexpected response
    #[error("protocol sequence error")]
    UnexpectedResponse(),
}

/// JSON RPC Protocol error
#[derive(Debug, thiserror::Error, serde::Deserialize, serde::Serialize)]
pub struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(default)]
    data: serde_json::Value,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JsonRpcError {{ code: {}, message: {}, data: {} }}",
            self.code, self.message, self.data
        )
    }
}

/// JSON RPC request
#[derive(Debug, serde::Serialize)]
pub struct JsonRpcRequest {
    jsonrpc: JsonRpcVersion,
    method: String,
    id: u64,
    params: serde_json::Value,
}

/// JSON RPC response
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct JsonRpcResponse {
    jsonrpc: JsonRpcVersion,
    id: u64,
    #[serde(flatten)]
    body: JsonRpcResponseBody,
}

/// JSON RPC Response body
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonRpcResponseBody {
    /// normal response
    #[serde(rename = "result")]
    Response(serde_json::Value),
    /// error response
    Error(JsonRpcError),
}

impl JsonRpcResponse {
    /// Internally allocated request ID.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Consume the response as an API result or server error.
    ///
    /// # Errors
    /// Returns the JSON-RPC error supplied by the server.
    pub fn into_result(self) -> Result<serde_json::Value, JsonRpcError> {
        self.body.into_result()
    }

    fn trace_received(&self) {
        if let Some(code) = self.body.error_code() {
            ::tracing::info!(request_id = self.id, error_code = code, "response received");
        } else {
            ::tracing::info!(request_id = self.id, "response received");
        }
    }
}

impl JsonRpcResponseBody {
    const fn error_code(&self) -> Option<i32> {
        match self {
            Self::Response(_) => None,
            Self::Error(error) => Some(error.code),
        }
    }
    fn into_result(self) -> Result<serde_json::Value, JsonRpcError> {
        match self {
            Self::Response(value) => Ok(value),
            Self::Error(error) => Err(error),
        }
    }
}

impl JsonRpcRequest {
    fn build_request(id: u64, method: &str, params: &serde_json::Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::default(),
            method: method.to_owned(),
            id,
            params: params.clone(),
        }
    }
}

/// JSON RPC version tag
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub enum JsonRpcVersion {
    /// version value for "2.0"
    #[default]
    #[serde(rename = "2.0")]
    Version2,
}

/// JSON RPC Client for TCP
#[derive(Debug)]
pub struct TcpJsonRpcClient {
    stream: std::net::TcpStream,
    buff_read: std::io::BufReader<TcpStream>,
    id: u64,
}

impl TcpJsonRpcClient {
    /// # Errors
    ///
    /// - `std::io::Error`
    pub fn try_from(stream: std::net::TcpStream) -> Result<Self, std::io::Error> {
        let buff_read = BufReader::new(stream.try_clone()?);
        Ok(Self {
            stream,
            buff_read,
            id: 0,
        })
    }

    const fn get_id(&mut self) -> u64 {
        let result = self.id;
        self.id += 1;
        result
    }

    /// # Errors
    ///
    /// - `JsonRpcClientError`
    pub fn invoke_method(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, JsonRpcClientError> {
        let id = self.get_id();
        let request = JsonRpcRequest::build_request(id, method, params);
        let mut json = serde_json::to_string(&request)?;
        json.push('\n');
        self.stream.write_all(json.as_bytes())?;

        let response: JsonRpcResponse = serde_json::from_reader(&mut self.buff_read)?;

        if response.id != id {
            Err(JsonRpcClientError::UnexpectedResponse())?;
        }

        Ok(response.body.into_result()?)
    }
}
