#![deny(missing_docs)]
#![doc = include_str!("lib.md")]

mod analyze_query;
mod collection_management;
mod describe_table;
mod initialize_schema;
mod list_objects;

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
    initialize_schema::{
        EngineDialect, EngineVersion, ExtensionSpec, InitializeSchemaArgs, SchemaContext,
    },
    list_objects::{
        ListObjectArgs, NameQuery, ObjectKind, ObjectQuery, ObjectSummary, ObjectSummaryField,
        StringPred,
    },
};

use std::path::PathBuf;

/// semantic analysis context handle
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ContextHandle(String);

/// position in SQL text
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
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TextSpan {
    /// span start position
    from: TextPosition,

    /// span end position
    to: TextPosition,
}

/// semantic analysis probrem
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalysisProbrem {
    /// error code
    code: String,

    /// file path
    file_path: PathBuf,

    /// statement span
    statement_span: TextSpan,

    /// probrem span
    probrem_span: TextSpan,
}

/// error for api
#[derive(thiserror::Error, Debug, serde::Serialize, serde::Deserialize)]
pub enum SqlFunServerApiError {
    /// require release collection
    ///
    /// server keeping to many collections for client, release collection for new listing/analyse request.
    #[error("require release collection {0:?}")]
    RequireReleseCollection(Vec<CollectionHandle>),
}

/// database type reference
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TypeReference {
    /// schema name
    schema_name: String,
    /// local name
    local_name: String,
}

/// nullability
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
    /// Initialize schema-aware query analysis context
    async fn initialize_schema_context(
        &self,
        args: InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerApiError>;

    /// Read a collection
    ///
    /// When server returns multiple items, server keeps scrollable collection,
    /// [`read_collection`] reads items in collection.
    ///
    /// # Return value
    ///
    /// returns [Collection<T>], T depends collection item type
    ///
    async fn read_collection(
        &self,
        args: ReadCollectionArgs,
    ) -> Result<serde_json::Value, SqlFunServerApiError>;

    /// Release a collection
    ///
    /// When server returns collection, server keeps scrollable collection,
    /// [`release_collection`] release a server side collection.
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
