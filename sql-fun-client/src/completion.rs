use crate::async_tokio::streaming::RequestSender;
use crate::payloads::{
    AnalysisProbrem, AnalyzeQueryItem, AnalyzeQueryResponse, CollectionHandle, ColumnAccess,
    ColumnDefinition, DescribeTableResponse, ObjectSummary, ParameterDescribe, ResultSetColumn,
    SchemaContext, TableConstraint,
};
use crate::{Collection, StreamingCollection};
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

impl<T: CompleteResponse + ::serde::de::DeserializeOwned + 'static> CompleteResponse
    for Collection<T>
{
    fn register_collections(&self, context: &mut CompletionContext) {
        if let Self::Stream(page) = self {
            context.retain(&page.handle, page.require_release);
        }
        for item in self.items() {
            item.register_collections(context);
        }
    }

    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
        Box::pin(async move {
            let page = match self {
                Self::Stream(page) => page,
                Self::Completed(items) => return items.complete(context).await,
            };
            context.retain(&page.handle, page.require_release);
            let initial_end = page.checked_end()?;
            let mut prefix = page.read_range(context, 0, page.offset).await?;
            let mut suffix = page
                .read_range(context, initial_end, page.total_length)
                .await?;
            prefix.append(&mut page.items);
            prefix.append(&mut suffix);
            prefix.register_collections(context);
            prefix.complete(context).await?;
            if page.require_release {
                context.release(&page.handle).await?;
            }
            *self = Self::completed(prefix);
            Ok(())
        })
    }
}

impl<T: CompleteResponse + ::serde::de::DeserializeOwned + 'static> StreamingCollection<T> {
    fn checked_end(&self) -> Result<usize, TypedResponseError> {
        let Some(end) = self.offset.checked_add(self.items.len()) else {
            return Err(TypedResponseError::message(
                "collection page range overflow",
            ));
        };
        if end > self.total_length {
            return Err(TypedResponseError::message(
                "collection page exceeds total length",
            ));
        }
        Ok(end)
    }

    async fn read_range(
        &mut self,
        context: &mut CompletionContext,
        mut offset: usize,
        end: usize,
    ) -> Result<Vec<T>, TypedResponseError> {
        let mut items = Vec::new();
        while offset < end {
            let params = ::serde_json::json!({
                "collection_handle": self.handle, "start_offset": offset, "count": end - offset
            });
            let value = context
                .sender
                .collection_rpc("read_collection", &params)
                .await?;
            // Decode the wire metadata before T, preserving the outer handle on type errors.
            let page: StreamingCollection<::serde_json::Value> = ::serde_json::from_value(value)?;
            if page.handle != self.handle
                || page.offset != offset
                || page.total_length != self.total_length
                || page.require_release != self.require_release
            {
                return Err(TypedResponseError::message(
                    "inconsistent collection continuation metadata",
                ));
            }
            let next = page.checked_end()?;
            if next > end || next == offset {
                return Err(TypedResponseError::message(
                    "collection continuation exceeds requested range or makes no progress",
                ));
            }
            for value in page.items {
                let item: T = ::serde_json::from_value(value)?;
                item.register_collections(context);
                items.push(item);
            }
            offset = next;
        }
        Ok(items)
    }
}

impl<T: CompleteResponse> CompleteResponse for Vec<T> {
    fn register_collections(&self, context: &mut CompletionContext) {
        for item in self {
            item.register_collections(context);
        }
    }
    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
        Box::pin(async move {
            for item in self {
                item.complete(context).await?;
            }
            Ok(())
        })
    }
}
impl<T: CompleteResponse> CompleteResponse for Option<T> {
    fn register_collections(&self, context: &mut CompletionContext) {
        if let Some(item) = self {
            item.register_collections(context);
        }
    }
    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
        Box::pin(async move {
            if let Some(item) = self {
                item.complete(context).await?;
            }
            Ok(())
        })
    }
}
impl<T: CompleteResponse> CompleteResponse for Box<T> {
    fn register_collections(&self, context: &mut CompletionContext) {
        self.as_ref().register_collections(context);
    }
    fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
        self.as_mut().complete(context)
    }
}

macro_rules! leaf_response {
    ($($ty:ty),* $(,)?) => { $(impl CompleteResponse for $ty {
        fn register_collections(&self, _: &mut CompletionContext) {}
        fn complete<'a>(&'a mut self, _: &'a mut CompletionContext) -> CompletionFuture<'a> { Box::pin(async { Ok(()) }) }
    })* };
}
leaf_response!(
    (),
    String,
    bool,
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    f32,
    f64,
    ::serde_json::Value,
    AnalysisProbrem,
    ColumnAccess,
    ColumnDefinition,
    ObjectSummary,
    ParameterDescribe,
    ResultSetColumn,
    TableConstraint
);

macro_rules! response_fields {
    ($ty:ty, $( $field:ident => $access:ident ),+ $(,)?) => {
        impl CompleteResponse for $ty {
            fn register_collections(&self, context: &mut CompletionContext) { $(self.$field().register_collections(context);)+ }
            fn complete<'a>(&'a mut self, context: &'a mut CompletionContext) -> CompletionFuture<'a> {
                Box::pin(async move { $(self.$access().complete(context).await?;)+ Ok(()) })
            }
        }
    };
}
response_fields!(SchemaContext, problems => problems_mut);
response_fields!(DescribeTableResponse, columns => columns_mut, constraints => constraints_mut);
response_fields!(AnalyzeQueryResponse, queries => queries_mut);
response_fields!(AnalyzeQueryItem, probrems => probrems_mut, accessing_relations => accessing_relations_mut,
    column_access => column_access_mut, parameters => parameters_mut, result_columns => result_columns_mut);
