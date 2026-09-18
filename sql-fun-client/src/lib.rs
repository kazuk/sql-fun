//! client for sql-fun-server

use std::{
    io::{BufReader, Write},
    net::TcpStream,
};

/// JSON RPC Client errors
#[derive(Debug, thiserror::Error)]
pub enum JsonRpcClientError {
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON RPC error
    #[error(transparent)]
    JsonRpcError(#[from] JsonRpcError),

    /// serde error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    /// unexpected response
    #[error("protocol sequence error")]
    UnexpectedResponse(),
}

/// JSON RPC Protocol error
#[derive(Debug, thiserror::Error, serde::Deserialize)]
pub struct JsonRpcError {
    code: i16,
    message: String,
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
    params: Vec<serde_json::Value>,
}

/// JSON RPC response
#[derive(Debug, serde::Deserialize)]
pub struct JsonRpcResponse {
    #[expect(dead_code)]
    jsonrpc: JsonRpcVersion,
    id: u64,
    #[serde(flatten)]
    body: JsonRpcResponseBody,
}

/// JSON RPC Response body
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonRpcResponseBody {
    /// normal response
    Response(serde_json::Value),
    /// error response
    Error(JsonRpcError),
}

impl JsonRpcRequest {
    fn build_request(id: u64, method: String, params: Vec<serde_json::Value>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::default(),
            method,
            id,
            params,
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
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcClientError> {
        let id = self.get_id();
        let request = JsonRpcRequest::build_request(id, String::from(method), params);
        let mut json = serde_json::to_string(&request)?;
        json.push('\n');
        self.stream.write_all(json.as_bytes())?;

        let response: JsonRpcResponse = serde_json::from_reader(&mut self.buff_read)?;

        if response.id != id {
            Err(JsonRpcClientError::UnexpectedResponse())?;
        }

        match response.body {
            JsonRpcResponseBody::Response(value) => Ok(value),
            JsonRpcResponseBody::Error(err) => Err(JsonRpcClientError::from(err)),
        }
    }
}

pub use async_tokio::{TcpJsonRpcClientAsync, TcpJsonRpcClientAsyncError};

mod async_tokio;

#[cfg(test)]
mod tests {}
