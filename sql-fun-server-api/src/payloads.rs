// The derive emits sibling implementations, outside the annotated type's lint scope.
#![cfg_attr(
    feature = "openrpc-schema",
    expect(
        clippy::disallowed_methods,
        reason = "schemars generates Option::and_then for serde defaults in derived schemas"
    )
)]

mod analyze_query;
mod describe_table;
mod echo;
mod initialize_schema;
mod list_objects;
mod traversal;

pub use self::traversal::{CollectionPage, CollectionReference, CollectionTraversal};

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
    /// Whether this is an array of the named type.
    #[serde(default)]
    is_array: bool,
}

/// Severity of a retained semantic diagnostic.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// SQL semantic error.
    Error,
    /// Accepted SQL with a warning.
    Warning,
    /// Informational advice.
    Advice,
}

impl DiagnosticSeverity {
    /// Construct an error severity.
    #[must_use]
    pub const fn error() -> Self {
        Self::Error
    }
    /// Construct a warning severity.
    #[must_use]
    pub const fn warning() -> Self {
        Self::Warning
    }
    /// Construct an advice severity.
    #[must_use]
    pub const fn advice() -> Self {
        Self::Advice
    }
}

impl From<::sql_fun_core::analysis_problem::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(value: ::sql_fun_core::analysis_problem::DiagnosticSeverity) -> Self {
        use ::sql_fun_core::analysis_problem::DiagnosticSeverity as CoreSeverity;
        match value {
            CoreSeverity::Error => Self::error(),
            CoreSeverity::Warning => Self::warning(),
            CoreSeverity::Advice => Self::advice(),
        }
    }
}

/// semantic analysis probrem
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalysisProbrem {
    /// Structured semantic problem when a reliable source presentation is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic_problem: Option<::sql_fun_core::analysis_problem::AnalysisProblem>,
    /// Structured diagnostic and analyzed source for optional CLI presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presentation: Option<::sql_fun_core::DiagnosticPresentation>,
    /// Diagnostic severity, retained independently of its code.
    severity: DiagnosticSeverity,

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
    /// Preserve the original semantic diagnostic without inventing source coordinates.
    #[must_use]
    pub fn with_semantic_problem(
        mut self,
        problem: &::sql_fun_core::analysis_problem::AnalysisProblem,
    ) -> Self {
        self.semantic_problem = Some(problem.clone());
        self
    }

    /// Attach presentation data without changing the existing summary fields.
    #[must_use]
    pub fn with_presentation(
        mut self,
        presentation: Option<&::sql_fun_core::DiagnosticPresentation>,
    ) -> Self {
        self.presentation = presentation.cloned();
        self
    }

    /// Construct a retained schema diagnostic without inventing missing locations.
    /// Takes ownership of the optional span to avoid copying it.
    #[must_use]
    pub fn new(
        code: &str,
        severity: DiagnosticSeverity,
        file_path: &std::path::Path,
        statement_relative_span: Option<StatementByteRange>,
    ) -> Self {
        Self {
            presentation: None,
            semantic_problem: None,
            severity,
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
            super::DiagnosticSeverity::error(),
            std::path::Path::new("schema.sql"),
            Some(StatementByteRange::new(9, 7)),
        );
        let value = ::serde_json::to_value(problem).unwrap();
        assert_eq!(
            value,
            ::serde_json::json!({
                "code":"SF000001", "severity":"error", "file_path":"schema.sql", "statement_span":null,
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

impl CollectionTraversal for AnalysisProbrem {}

impl TypeReference {
    /// Construct a named SQL type, preserving array identity.
    #[must_use]
    pub fn new(schema_name: &str, local_name: &str, is_array: bool) -> Self {
        Self {
            schema_name: schema_name.to_owned(),
            local_name: local_name.to_owned(),
            is_array,
        }
    }
}
impl TextSpan {
    /// Range in UTF-8 bytes of the explicit SQL source. Lines and columns are one-based.
    #[must_use]
    pub fn from_offsets(source: &str, from: usize, to: usize) -> Self {
        Self {
            from: TextPosition::at(source, from),
            to: TextPosition::at(source, to),
        }
    }
}
impl TextPosition {
    fn at(source: &str, offset: usize) -> Self {
        let prefix = &source[..offset];
        Self {
            offset,
            line: u32::try_from(prefix.bytes().filter(|b| *b == b'\n').count() + 1)
                .expect("parsed SQL line fits u32"),
            col: u32::try_from(prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1)
                .expect("parsed SQL column fits u32"),
        }
    }
}
impl Nullability {
    /// Preserve the analyzer's three-state nullability.
    #[must_use]
    pub const fn from_not_null(value: Option<bool>) -> Self {
        match value {
            Some(true) => Self::NotNull,
            Some(false) => Self::Nullable,
            None => Self::Unknown,
        }
    }
}

impl ReadCollectionArgs {
    /// Read a bounded page of an existing session collection.
    #[must_use]
    pub fn new(handle: &CollectionHandle, start_offset: usize, count: Option<usize>) -> Self {
        Self {
            collection_handle: handle.clone(),
            start_offset,
            count,
        }
    }
}
impl ReleaseCollectionArgs {
    /// Release an existing session collection.
    #[must_use]
    pub fn new(handle: &CollectionHandle) -> Self {
        Self {
            collection_handle: handle.clone(),
        }
    }
}
