#![deny(missing_docs)]
#![doc = include_str!("lib.md")]

mod collection_management;

/// payload object definitions
pub mod payloads;

use crate::payloads::{
    AnalyzeQueryArgs, AnalyzeQueryResponse, CollectionHandle, DescribeTableArgs,
    DescribeTableResponse, EchoArgs, InitializeSchemaArgs, ListObjectArgs, ObjectSummary,
    ReadCollectionArgs, ReleaseCollectionArgs, SchemaContext,
};

pub use self::collection_management::Collection;

#[cfg(feature = "openrpc-schema")]
pub use self::rpc_schema::register_methods;

use std::{convert::Infallible, error::Error as StdError};

/// error for api
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(thiserror::Error, Debug, serde::Serialize, serde::Deserialize)]
pub enum SqlFunServerApiError {
    /// require release collection
    ///
    /// server keeping to many collections for client, release collection for new listing/analyse request.
    #[error("require release collection {0:?}")]
    RequireReleaseCollection(Vec<CollectionHandle>),

    /// Unknown, foreign-session, or already released collection handle.
    #[error("invalid collection handle")]
    InvalidCollectionHandle,

    /// Generic unexpected error
    #[error("unexpected {0}")]
    Unexpected(String),

    /// schema context initialize error
    #[error("schema context initialize failed : {0}")]
    SchemaContextInitializeFailed(String),
}

impl SqlFunServerApiError {
    /// Construct an invalid collection handle error without revealing its owner.
    #[must_use]
    pub const fn invalid_collection_handle() -> Self {
        Self::InvalidCollectionHandle
    }

    /// raise a [`Self::Unexpected`] error
    ///
    /// # Errors
    ///
    /// always raise a [`Self::Unexpected`] error
    ///
    pub fn raise_unexpected(message: &str) -> Result<Infallible, Self> {
        Err(Self::new_unexpected(message))
    }

    /// new [`Self::Unexpected`] error instance
    #[must_use]
    pub fn new_unexpected(message: &str) -> Self {
        Self::Unexpected(String::from(message))
    }

    /// raise a [`Self::SchemaContextInitializeFailed`] error
    ///
    /// # Errors
    ///
    /// always raize a [`Self::SchemaContextInitializeFailed`] error
    ///
    pub fn raise_schema_context_initialize_failed<E>(error: E) -> Result<Infallible, Self>
    where
        E: StdError,
    {
        Err(Self::new_schema_context_initialize_failed(error))
    }

    /// create new [`Self::SchemaContextInitializeFailed`] error instance
    #[must_use]
    pub fn new_schema_context_initialize_failed<E>(error: E) -> Self
    where
        E: StdError,
    {
        let message = format!("{error}");
        Self::SchemaContextInitializeFailed(message)
    }
}

/// `sql-fun` Server API functions
#[async_trait::async_trait]
pub trait SqlFunServerApi {
    /// Return object or array parameters unchanged for transport diagnostics.
    async fn echo(&self, args: EchoArgs) -> Result<serde_json::Value, SqlFunServerApiError>;

    /// Initialize schema-aware query analysis context
    async fn initialize_schema_context(
        &self,
        args: InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerApiError>;

    /// Read a collection
    ///
    /// When server returns multiple items, server keeps scrollable collection,
    /// [`SqlFunServerApi::read_collection`] reads items in collection.
    ///
    /// # Return value
    ///
    /// returns [`Collection<T>`], T depends collection item type
    ///
    async fn read_collection(
        &self,
        args: ReadCollectionArgs,
    ) -> Result<serde_json::Value, SqlFunServerApiError>;

    /// Release a collection
    ///
    /// When server returns collection, server keeps scrollable collection,
    /// [`SqlFunServerApi::release_collection`] release a server side collection.
    ///
    async fn release_collection(
        &self,
        args: ReleaseCollectionArgs,
    ) -> Result<(), SqlFunServerApiError>;

    /// analyze query
    async fn analyze_query(
        &self,
        args: AnalyzeQueryArgs,
    ) -> Result<AnalyzeQueryResponse, SqlFunServerApiError>;

    /// list schema objects
    async fn list_objects(
        &self,
        args: ListObjectArgs,
    ) -> Result<Collection<ObjectSummary>, SqlFunServerApiError>;

    /// describe a table
    async fn describe_table(
        &self,
        args: DescribeTableArgs,
    ) -> Result<DescribeTableResponse, SqlFunServerApiError>;
}

#[cfg(feature = "openrpc-schema")]
mod rpc_schema {

    /// JSON-RPC method definition for `initialize_schema_context` method
    use typed_openrpc::RpcMethod;

    pub struct InitializeSchemaContextMethod;

    impl RpcMethod for InitializeSchemaContextMethod {
        const NAME: &'static str = "initialize_schema_context";
        const SUMMARY: &'static str = "load schema dump file for semantic analysis";

        type Params = super::InitializeSchemaArgs;
        type Result = super::SchemaContext;
    }

    pub struct EchoMethod;

    impl RpcMethod for EchoMethod {
        const NAME: &'static str = "echo";
        const SUMMARY: &'static str = "return parameters unchanged for diagnostics";

        type Params = super::EchoArgs;
        type Result = super::EchoArgs;
    }

    pub struct ReadCollectionMethod;

    impl RpcMethod for ReadCollectionMethod {
        const NAME: &'static str = "read_collection";
        const SUMMARY: &'static str = "read a collection";

        type Params = super::ReadCollectionArgs;
        type Result = super::Collection<serde_json::Value>;
    }

    pub struct ReleaseCollectionMethod;

    impl RpcMethod for ReleaseCollectionMethod {
        const NAME: &'static str = "release_collection";
        const SUMMARY: &'static str = "release a collection";

        type Params = super::ReleaseCollectionArgs;
        type Result = ();
    }

    pub struct AnalyzeQueryMethod;

    impl RpcMethod for AnalyzeQueryMethod {
        const NAME: &'static str = "analyze_query";
        const SUMMARY: &'static str = "analyze a query";

        type Params = super::AnalyzeQueryArgs;
        type Result = super::AnalyzeQueryResponse;
    }

    pub struct ListObjectMethod;

    impl RpcMethod for ListObjectMethod {
        const NAME: &'static str = "list_object";
        const SUMMARY: &'static str = "list schema objects";

        type Params = super::ListObjectArgs;
        type Result = super::Collection<super::ObjectSummary>;
    }

    /// register all JSON-RPC methods
    pub fn register_methods(registry: &mut typed_openrpc::Registry) {
        registry.register_method::<EchoMethod>();
        registry.register_method::<InitializeSchemaContextMethod>();
        registry.register_method::<ReadCollectionMethod>();
        registry.register_method::<ReleaseCollectionMethod>();
        registry.register_method::<AnalyzeQueryMethod>();
        registry.register_method::<ListObjectMethod>();
    }
}
