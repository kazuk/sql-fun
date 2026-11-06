#![deny(missing_docs)]
#![doc = include_str!("lib.md")]

mod collection_management;

/// payload object definitions
pub mod payloads;

pub use self::collection_management::Collection;
#[cfg(feature = "openrpc-schema")]
pub use self::rpc_schema::register_methods;

use crate::payloads::CollectionHandle;

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

    /// Unknown or foreign-session schema context.
    #[error("invalid context handle")]
    InvalidContextHandle,

    /// Invalid or unsupported analysis request input.
    #[error("invalid analysis input: {0}")]
    InvalidAnalysisInput(String),

    /// The selected lexer rejected the original source.
    #[error("SQL tokenization failed: {0}")]
    TokenizationFailed(String),

    /// Generic unexpected error
    #[error("unexpected {0}")]
    Unexpected(String),

    /// schema context initialize error
    #[error("schema context initialize failed : {0}")]
    SchemaContextInitializeFailed(String),
}

impl SqlFunServerApiError {
    /// Retain a lexical failure separately from semantic diagnostics.
    #[must_use]
    pub fn tokenization_failed(message: &str) -> Self {
        Self::TokenizationFailed(message.to_owned())
    }

    /// Unknown context without disclosing its owner.
    #[must_use]
    pub const fn invalid_context_handle() -> Self {
        Self::InvalidContextHandle
    }
    /// Invalid or unsupported input selection.
    #[must_use]
    pub fn invalid_analysis_input(message: &str) -> Self {
        Self::InvalidAnalysisInput(message.to_owned())
    }

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

#[cfg(feature = "openrpc-schema")]
mod rpc_schema {
    use crate::{
        Collection,
        payloads::{
            AnalyzeQueryArgs, AnalyzeQueryResponse, EchoArgs, InitializeSchemaArgs,
            ReadCollectionArgs, ReleaseCollectionArgs, SchemaContext, SqlToken, TokenizeSqlArgs,
        },
    };
    use ::typed_openrpc::RpcMethod;

    pub struct InitializeSchemaContextMethod;

    impl RpcMethod for InitializeSchemaContextMethod {
        const NAME: &'static str = "initialize_schema_context";
        const SUMMARY: &'static str = "load schema dump file for semantic analysis";

        type Params = InitializeSchemaArgs;
        type Result = SchemaContext;
    }

    pub struct TokenizeSqlMethod;
    impl RpcMethod for TokenizeSqlMethod {
        const NAME: &'static str = "tokenize_sql";
        const SUMMARY: &'static str = "tokenize original SQL without schema analysis";
        type Params = TokenizeSqlArgs;
        type Result = Collection<SqlToken>;
    }

    pub struct EchoMethod;

    impl RpcMethod for EchoMethod {
        const NAME: &'static str = "echo";
        const SUMMARY: &'static str = "return parameters unchanged for diagnostics";

        type Params = EchoArgs;
        type Result = EchoArgs;
    }

    pub struct ReadCollectionMethod;

    impl RpcMethod for ReadCollectionMethod {
        const NAME: &'static str = "read_collection";
        const SUMMARY: &'static str = "read a collection";

        type Params = ReadCollectionArgs;
        type Result = Collection<serde_json::Value>;
    }

    pub struct ReleaseCollectionMethod;

    impl RpcMethod for ReleaseCollectionMethod {
        const NAME: &'static str = "release_collection";
        const SUMMARY: &'static str = "release a collection";

        type Params = ReleaseCollectionArgs;
        type Result = ();
    }

    pub struct AnalyzeQueryMethod;

    impl RpcMethod for AnalyzeQueryMethod {
        const NAME: &'static str = "analyze_query";
        const SUMMARY: &'static str = "analyze a query";

        type Params = AnalyzeQueryArgs;
        type Result = AnalyzeQueryResponse;
    }

    /// register all JSON-RPC methods
    pub fn register_methods(registry: &mut typed_openrpc::Registry) {
        registry.register_method::<EchoMethod>();
        registry.register_method::<TokenizeSqlMethod>();
        registry.register_method::<InitializeSchemaContextMethod>();
        registry.register_method::<ReadCollectionMethod>();
        registry.register_method::<ReleaseCollectionMethod>();
        registry.register_method::<AnalyzeQueryMethod>();
    }
}

#[cfg(all(test, feature = "openrpc-schema"))]
mod test_tokenize_openrpc {
    use crate::register_methods;

    #[test]
    fn registers_tokenization_with_required_inputs_and_collection_result() {
        let mut registry = ::typed_openrpc::Registry::new();
        register_methods(&mut registry);
        let info = ::typed_openrpc::OpenRpcInfo {
            title: "test".to_owned(),
            version: "test".to_owned(),
            description: None,
        };
        let doc = ::serde_json::to_value(registry.generate_openrpc_doc_with_info(&info)).unwrap();
        let mut found = false;
        for method in doc["methods"].as_array().unwrap() {
            if method["name"] == "tokenize_sql" {
                found = true;
                let json = method.to_string();
                assert!(json.contains("source"));
                assert!(json.contains("dialect"));
                assert!(json.contains("SqlToken"));
                assert!(method.get("result").is_some());
            }
        }
        assert!(found, "tokenize_sql missing from OpenRPC");
    }
}
