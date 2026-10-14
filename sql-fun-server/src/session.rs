mod collection_registry;
mod context_handle_registry;

use self::{
    collection_registry::CollectionRegistry, context_handle_registry::ContextHandleRegistry,
};

use crate::{
    ServerError,
    buffers::JsonLinesStreamReader,
    json_rpc::{JsonRpcRequest, JsonRpcResponse},
    process_request,
};

use std::{net::SocketAddr, num::NonZeroUsize};

use ::tokio::{
    io::AsyncWriteExt,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

/// Per-authenticated-connection owner for subsequently retained schema contexts.
///
/// Future borrowing session state belongs inside the handler, so it is dropped
/// before the wrapper's context owners rather than forming a self-reference.
/// Its cursors may borrow schemas for the registry's lifetime. The future RPC
/// facade must keep that lifetime separate from its shorter mutable-state borrow;
/// registration through a shared registry borrow cannot invalidate live cursors.
pub(super) struct SessionWrapper {
    contexts: ContextHandleRegistry,
}

impl SessionWrapper {
    pub(super) fn new() -> Self {
        Self {
            contexts: ContextHandleRegistry::new(),
        }
    }

    /// Run authenticated requests in the connection task using its existing reader.
    /// Normal EOF flushes and shuts down the writer; errors and cancellation keep
    /// the existing early-return and transport-drop behavior.
    pub(super) async fn session_handler(
        &self,
        mut request_reader: JsonLinesStreamReader<OwnedReadHalf>,
        mut write_half: OwnedWriteHalf,
        addr: SocketAddr,
        request_length_limit: usize,
    ) -> Result<(), ServerError> {
        // Handler-local storage drops before the wrapper's strong schema owners,
        // including on early errors and task cancellation. RPC integration follows.
        let _collections = CollectionRegistry::new(NonZeroUsize::new(1_024).unwrap());
        request_reader
            .set_request_length_limit(request_length_limit)
            .await;

        while let Some(request) = request_reader.read_request().await? {
            let request_message: JsonRpcRequest = ::serde_json::from_value(request)?;

            let response = process_request(&request_message).await;
            let response = match response {
                Ok(result) => JsonRpcResponse::success(&request_message, result),
                Err(error) => JsonRpcResponse::error(&request_message, &error),
            };
            let mut response_json = ::serde_json::to_string(&response)?;
            response_json.push('\n');

            ::tracing::info!("{addr}: sending {response_json}");
            write_half.write_all(response_json.as_bytes()).await?;
        }
        write_half.flush().await?;
        write_half.shutdown().await?;
        Ok(())
    }
}
