#![deny(missing_docs)]
#![doc = include_str!("lib.md")]

mod analyze_query;
mod collection_management;
mod describe_table;
mod echo;
mod initialize_schema;
mod list_objects;

#[cfg(feature = "openrpc-schema")]
pub use self::rpc_schema::register_methods;
pub use self::{
    analyze_query::{
        AnalyzeQueryArgs, AnalyzeQueryInput, AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess,
        ColumnAccessKind, ParameterDescribe, QueryKind, ResultSetColumn,
    },
    collection_management::{
        Collection, CollectionHandle, ReadCollectionArgs, ReleaseCollectionArgs,
    },
    describe_table::{
        ColumnConstraint, ColumnDefinition, DescribeTableArgs, DescribeTableResponse,
        TableConstraint,
    },
    echo::EchoArgs,
    initialize_schema::{
        EngineDialect, EngineVersion, ExtensionSpec, InitializeSchemaArgs, SchemaContext,
    },
    list_objects::{
        ListObjectArgs, NameQuery, ObjectKind, ObjectQuery, ObjectSummary, ObjectSummaryField,
        StringPred,
    },
};

use std::{convert::Infallible, error::Error as StdError, path::PathBuf};

/// Opaque identifier for a connection's registered semantic analysis context.
///
/// Constructing a handle does not register a context or grant access. Handles
/// resolve only in the connection that issued them.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ContextHandle(String);

impl From<String> for ContextHandle {
    /// Wrap a wire value without registering or validating a context.
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod test_context_handle {
    use crate::ContextHandle;
    use std::collections::HashMap;

    #[rstest::rstest]
    #[case::opaque("context-42")]
    #[case::empty("")]
    #[case::arbitrary("unknown handle / 日本語")]
    fn preserves_string_representation(#[case] value: &str) -> testresult::TestResult {
        let handle = ContextHandle::from(value.to_owned());
        let json = ::serde_json::to_string(&handle)?;
        assert_eq!(json, ::serde_json::to_string(value)?);
        let decoded: ContextHandle = ::serde_json::from_str(&json)?;
        let mut registered = HashMap::new();
        registered.insert(handle.clone(), 42);
        assert!(handle == decoded);
        assert_eq!(registered.get(&decoded), Some(&42));
        Ok(())
    }
}

/// position in SQL text
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TextPosition {
    /// seek offset from file start
    offset: usize,

    /// line number
    line: u32,

    /// column number
    col: u32,
}

#[cfg(test)]
mod test_text_position {
    use testresult::TestResult;

    use crate::TextPosition;

    #[test]
    fn test_serialize() -> TestResult {
        let item = TextPosition {
            offset: 0,
            line: 1,
            col: 2,
        };
        let json_string = serde_json::to_string(&item)?;
        assert_eq!(&json_string, r#"{"offset":0,"line":1,"col":2}"#);
        Ok(())
    }
}

/// span in SQL text
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TextSpan {
    /// span start position
    from: TextPosition,

    /// span end position
    to: TextPosition,
}

/// semantic analysis probrem
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalysisProbrem {
    /// error code
    code: String,

    /// file path
    file_path: PathBuf,

    /// Statement span, or None when not retained by analysis.
    statement_span: Option<TextSpan>,

    /// Problem span, or None when not retained by analysis.
    probrem_span: Option<TextSpan>,

    /// Byte range relative to the SQL statement, when a file position is unavailable.
    statement_relative_span: Option<StatementByteRange>,
}

/// A retained byte range within a SQL statement, not a file position.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StatementByteRange {
    /// UTF-8 byte offset from the SQL statement start.
    offset: usize,
    /// Number of bytes in the retained range.
    length: usize,
}

impl StatementByteRange {
    /// Construct a statement-relative range from retained analysis metadata.
    #[must_use]
    pub const fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    /// UTF-8 byte offset from the statement start.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Number of bytes in the range.
    #[must_use]
    pub const fn length(&self) -> usize {
        self.length
    }
}

impl AnalysisProbrem {
    /// Construct a retained schema diagnostic without inventing missing locations.
    /// Takes ownership of the optional span to avoid copying it.
    #[must_use]
    pub fn new(
        code: &str,
        file_path: &std::path::Path,
        statement_relative_span: Option<StatementByteRange>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            file_path: file_path.to_path_buf(),
            statement_span: None,
            probrem_span: None,
            statement_relative_span,
        }
    }
}

#[cfg(test)]
mod test_retained_diagnostic {
    use super::{AnalysisProbrem, StatementByteRange};

    #[test]
    fn retains_byte_offsets_without_inventing_line_or_statement_positions() {
        let problem = AnalysisProbrem::new(
            "SF000001",
            std::path::Path::new("schema.sql"),
            Some(StatementByteRange::new(9, 7)),
        );
        let value = ::serde_json::to_value(problem).unwrap();
        assert_eq!(
            value,
            ::serde_json::json!({
                "code":"SF000001", "file_path":"schema.sql", "statement_span":null,
                "probrem_span":null,
                "statement_relative_span":{"offset":9,"length":7}
            })
        );
        let _: AnalysisProbrem = ::serde_json::from_value(value).unwrap();
    }

    #[cfg(feature = "openrpc-schema")]
    #[test]
    fn schema_supports_unavailable_diagnostic_locations() {
        let schema = ::serde_json::to_value(::schemars::schema_for!(AnalysisProbrem)).unwrap();
        for field in ["statement_span", "probrem_span"] {
            assert!(
                schema["properties"][field]["anyOf"]
                    .as_array()
                    .unwrap()
                    .contains(&::serde_json::json!({"type":"null"}))
            );
        }
    }
}

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

/// database type reference
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TypeReference {
    /// schema name
    schema_name: String,
    /// local name
    local_name: String,
}

/// nullability
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Nullability {
    /// not null
    NotNull,
    /// can be null
    Nullable,
    /// unknown
    Unknown,
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
