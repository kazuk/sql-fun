use std::path::PathBuf;

use crate::payloads::TextSpan;

/// filter condition for string
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum StringPred {
    /// exact matches string
    Exact(String),
    /// match prefix
    StartsWith(String),
    /// match postfix
    EndsWith(String),
}

/// filter condition for object name
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum NameQuery {
    /// local name
    LocalName(StringPred),
    /// shcema name
    SchemaName(StringPred),
}

/// object kind.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ObjectKind {
    /// All object kind
    ///
    /// Used only in object query predicate.
    All,
    /// table
    Table,
    /// view
    View,
    /// type
    Type,
    /// function
    Function,
    /// materialized view
    MaterializedView,
    /// sequence
    Sequence,
    /// trigger
    Trigger,
    /// index
    Index,
}

/// filter conditions
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ObjectQuery {
    /// object kind
    Kind(ObjectKind),
    /// object schema name, local name predicate
    Name(NameQuery),
    /// filter any condition
    Or(Vec<Self>),
    /// filter all condition
    And(Vec<Self>),
    /// negate condition
    Not(Box<Self>),
}

/// specify optional field to returns API result
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ObjectSummaryField {
    /// returns `definition_file` field
    DefinitionFile,
    /// returns `definition_span` field
    DefinitionSpan,
}

/// argument for `list_objects`
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ListObjectArgs {
    /// predicate
    predicate: ObjectQuery,
    /// extra field
    extra_fields: Vec<ObjectSummaryField>,
}

/// database object summary
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ObjectSummary {
    /// schema name
    schema_name: String,
    /// local name
    local_name: String,
    /// object kind
    kind: ObjectKind,
    /// definition SQL file
    definition_file: Option<PathBuf>,
    /// definition statement span
    definition_span: Option<TextSpan>,
}
