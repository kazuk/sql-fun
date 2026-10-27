use crate::StreamingCollection;
use crate::async_tokio::streaming::RequestSender;
use crate::payloads::{CollectionHandle, CollectionPage, CollectionReference, CollectionTraversal};
use std::{collections::HashMap, fmt::Write, future::Future, pin::Pin};

/// A response completion operation, including recursively nested collection items.
pub type CompletionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), TypedResponseError>> + Send + 'a>>;

/// Type-directed traversal for the explicitly selected typed response API.
///
/// Implement both methods for custom response types: register every known handle
/// before completing fields, then delegate completion to each collection-bearing
/// field. Leaf types explicitly implement a no-op completion. JSON values are
/// opaque leaves; collection-shaped JSON objects are never inferred as handles.
pub trait CompleteResponse: Send {
    /// Register all presently decoded retained handles for failure cleanup.
    fn register_collections(&self, context: &mut CompletionContext);
    /// Complete all contained collections, propagating any failure.
    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a>;
}

/// Failure to decode, materialize, or release a typed response.
#[derive(Debug, thiserror::Error)]
#[error("{message}{cleanup}")]
pub struct TypedResponseError {
    message: String,
    cleanup: String,
}

impl TypedResponseError {
    /// Describe a custom response completion failure.
    #[must_use]
    pub fn message(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            cleanup: String::new(),
        }
    }
}

impl From<::serde_json::Error> for TypedResponseError {
    fn from(error: ::serde_json::Error) -> Self {
        Self::message(&error.to_string())
    }
}
impl From<crate::JsonRpcError> for TypedResponseError {
    fn from(error: crate::JsonRpcError) -> Self {
        Self::message(&error.to_string())
    }
}
impl From<crate::TcpJsonRpcClientAsyncError> for TypedResponseError {
    fn from(error: crate::TcpJsonRpcClientAsyncError) -> Self {
        Self::message(&error.to_string())
    }
}

/// Owns same-session continuation and best-effort cleanup for one typed response.
///
/// Constructed by the client. Custom response implementations only delegate to
/// their fields; they do not manage request permits or transport primitives.
pub struct CompletionContext {
    sender: RequestSender,
    retained: HashMap<CollectionHandle, bool>,
}

impl CompletionContext {
    pub(crate) fn new(sender: RequestSender) -> Self {
        Self {
            sender,
            retained: HashMap::new(),
        }
    }

    fn retain(&mut self, handle: &CollectionHandle, required: bool) {
        if required {
            self.retained.entry(handle.clone()).or_insert(false);
        }
    }

    async fn release(&mut self, handle: &CollectionHandle) -> Result<(), TypedResponseError> {
        let Some(attempted) = self.retained.get_mut(handle) else {
            return Ok(());
        };
        if *attempted {
            return Err(TypedResponseError::message(
                "collection handle already released or release attempted",
            ));
        }
        // Mark before sending: a failed transport may leave the outcome uncertain.
        *attempted = true;
        let value = self
            .sender
            .collection_rpc(
                "release_collection",
                &::serde_json::json!({"collection_handle": handle}),
            )
            .await?;
        ::serde_json::from_value::<()>(value)?;
        Ok(())
    }

    pub(crate) async fn finish<R>(
        &mut self,
        result: Result<R, TypedResponseError>,
    ) -> Result<R, TypedResponseError> {
        let mut error = match result {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if self.sender.is_open() {
            let handles: Vec<_> = self
                .retained
                .iter()
                .filter(|(_, attempted)| !**attempted)
                .map(|(handle, _)| handle.clone())
                .collect();
            for handle in handles {
                if !self.sender.is_open() {
                    break;
                }
                if let Err(cleanup) = self.release(&handle).await {
                    let _ = write!(error.cleanup, "; cleanup failed: {cleanup}");
                }
            }
        }
        Err(error)
    }
}

impl<T: CollectionTraversal> CompleteResponse for T {
    fn register_collections(&self, context: &mut CompletionContext) {
        for collection in self.collections() {
            context.register(collection);
        }
    }
    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
        Box::pin(async move {
            for collection in self.collections_mut() {
                context.complete_collection(collection).await?;
            }
            Ok(())
        })
    }
}

impl CompletionContext {
    fn register(&mut self, collection: &dyn CollectionReference) {
        if let Some(page) = collection.page() {
            self.retain(page.handle(), *page.require_release());
        }
        for child in collection.children() {
            self.register(child);
        }
    }

    fn complete_collection<'a>(
        &'a mut self,
        collection: &'a mut dyn CollectionReference,
    ) -> CompletionFuture<'a> {
        Box::pin(async move {
            if let Some(page) = collection.page() {
                let initial_end =
                    Self::checked_end(*page.offset(), *page.length(), *page.total_length())?;
                self.read_range(collection, &page, 0, *page.offset())
                    .await?;
                self.read_range(collection, &page, initial_end, *page.total_length())
                    .await?;
                for child in collection.children_mut() {
                    self.complete_collection(child).await?;
                }
                if *page.require_release() {
                    self.release(page.handle()).await?;
                }
                collection.finish();
            } else {
                for child in collection.children_mut() {
                    self.complete_collection(child).await?;
                }
            }
            Ok(())
        })
    }

    fn checked_end(
        offset: usize,
        length: usize,
        total: usize,
    ) -> Result<usize, TypedResponseError> {
        let Some(end) = offset.checked_add(length) else {
            return Err(TypedResponseError::message(
                "collection page range overflow",
            ));
        };
        if end > total {
            return Err(TypedResponseError::message(
                "collection page exceeds total length",
            ));
        }
        Ok(end)
    }

    async fn read_range(
        &mut self,
        collection: &mut dyn CollectionReference,
        initial: &CollectionPage,
        mut offset: usize,
        end: usize,
    ) -> Result<(), TypedResponseError> {
        while offset < end {
            let params = ::serde_json::json!({"collection_handle": initial.handle(), "start_offset": offset, "count": end - offset});
            let value = self
                .sender
                .collection_rpc("read_collection", &params)
                .await?;
            let page: StreamingCollection<::serde_json::Value> = ::serde_json::from_value(value)?;
            if page.handle() != initial.handle()
                || *page.offset() != offset
                || page.total_length() != initial.total_length()
                || page.require_release() != initial.require_release()
            {
                return Err(TypedResponseError::message(
                    "inconsistent collection continuation metadata",
                ));
            }
            let next = Self::checked_end(offset, page.items().len(), *page.total_length())?;
            if next > end || next == offset {
                return Err(TypedResponseError::message(
                    "collection continuation exceeds requested range or makes no progress",
                ));
            }
            let decoded = collection.insert(offset, page.into_items());
            // Even a partially decoded page can introduce handles needing cleanup.
            self.register(collection);
            decoded?;
            offset = next;
        }
        Ok(())
    }
}
