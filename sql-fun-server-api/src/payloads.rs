mod analyze_query;
mod describe_table;
mod echo;
mod initialize_schema;
mod list_objects;

use std::path::PathBuf;

pub use self::{
    analyze_query::{
        AnalyzeQueryArgs, AnalyzeQueryInput, AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess,
        ColumnAccessKind, ParameterDescribe, QueryKind, ResultSetColumn,
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

/// span in SQL text
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TextSpan {
    /// span start position
    from: TextPosition,

    /// span end position
    to: TextPosition,
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

    use super::TextPosition;

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

/// database type reference
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct TypeReference {
    /// schema name
    schema_name: String,
    /// local name
    local_name: String,
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
    use crate::payloads::ContextHandle;
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

/// collection handle
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CollectionHandle(String);

impl From<String> for CollectionHandle {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// argument for `release_collection`
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReleaseCollectionArgs {
    /// releasing collection handle
    collection_handle: CollectionHandle,
}

/// argument for `read_collection`
///
/// The item type returned by `read_collection` is identical to the element type
/// of the original `Collection<T>` that produced the handle, and never changes.
/// This method only reads the continuation of that collection; no type conversion
/// is performed.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReadCollectionArgs {
    /// collection
    collection_handle: CollectionHandle,

    /// start offset
    start_offset: usize,

    /// read count
    ///
    /// when None, client requests read to end, but server implementation has a hard limit
    count: Option<usize>,
}
