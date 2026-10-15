use std::path::PathBuf;

use url::Url;

use crate::{
    Collection,
    payloads::{
        AnalysisProbrem, ContextHandle, Nullability, ObjectKind, ObjectSummary, TextSpan,
        TypeReference,
    },
};

/// SQL file input source
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum AnalyzeQueryInput {
    /// actual text
    Text(String),
    /// local file
    File(PathBuf),
    /// remote file
    Url(Url),
}

/// `analyze_query` request arguments
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AnalyzeQueryArgs {
    /// specify semantic analysis context
    context_handle: ContextHandle,
    /// Query input source
    query: AnalyzeQueryInput,
}

/// response for `analyze_query`
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalyzeQueryResponse {
    queries: Collection<AnalyzeQueryItem>,
}

/// kind of semantic analyzed query
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum QueryKind {
    /// select statement
    Select,
    /// insert statement
    Insert,
    /// update statement
    Update,
    /// delete statement
    Delete,
    /// truncate statement
    Truncate,
}

/// analyzed single query
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalyzeQueryItem {
    /// query span in input
    query_span: TextSpan,

    /// query statement kind
    query_kind: QueryKind,

    /// semantic probrems
    probrems: Collection<AnalysisProbrem>,

    /// query accessing relations
    accessing_relations: Collection<ObjectSummary>,

    /// column access collection
    column_access: Collection<ColumnAccess>,

    /// query parameters
    parameters: Collection<ParameterDescribe>,

    /// query result sets
    result_columns: Collection<ResultSetColumn>,
}

/// parameter spec
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ParameterDescribe {
    /// name for parameter
    name: String,
    /// parameter usage in SQL text
    spans: Vec<TextSpan>,
    /// type candidates for this parameter
    type_candidate: Vec<TypeReference>,
    /// parameter nullability
    nullability: Nullability,
}

/// column access kind
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ColumnAccessKind {
    /// Column accessed in select statement column list.
    SelectResTarget,

    /// Column accessed in select statement from clause
    SelectFrom,

    /// Column accessed in select statement where clause.
    SelectWhere,

    /// column accessed in select statement order by clause
    SelectOrderBy,

    /// column accessed in select statement group by clause
    SelectGroupBy,

    /// column accessed in select statement having clause
    SelectHaving,

    /// column accessed in update statement set left value
    UpdateSet,

    /// column accessed in update statement set right value
    UpdateSetValue,

    /// column accessed in update statement from clause
    UpdateFrom,

    /// column accessed in update statement where clause
    UpdateWhere,

    /// column accessed in insert statement
    InsertInto,

    /// column accessed in delete statement
    Delete,
}

/// column access
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ColumnAccess {
    /// accessing span
    accessing_span: TextSpan,

    /// relation schema name
    relation_schema: String,

    /// relation local name
    relation_local_name: String,

    /// relation kind
    relation_kind: ObjectKind,

    /// column name
    column_name: String,

    /// column ordinal in relation
    column_ordinal: usize,

    /// kind of column access
    access_kind: ColumnAccessKind,
}

/// result set column description item
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ResultSetColumn {
    /// column ordinal
    ordinal: u32,
    /// column name
    name: String,
    /// column type
    column_type: Option<TypeReference>,
    /// nullability
    nullability: Nullability,
}

impl AnalyzeQueryResponse {
    /// Mutably access the queries collection.
    pub const fn queries_mut(&mut self) -> &mut Collection<AnalyzeQueryItem> {
        &mut self.queries
    }
}

impl AnalyzeQueryItem {
    /// Mutably access the probrems collection.
    pub const fn probrems_mut(&mut self) -> &mut Collection<AnalysisProbrem> {
        &mut self.probrems
    }
    /// Mutably access the accessing relations collection.
    pub const fn accessing_relations_mut(&mut self) -> &mut Collection<ObjectSummary> {
        &mut self.accessing_relations
    }
    /// Mutably access the column access collection.
    pub const fn column_access_mut(&mut self) -> &mut Collection<ColumnAccess> {
        &mut self.column_access
    }
    /// Mutably access the parameters collection.
    pub const fn parameters_mut(&mut self) -> &mut Collection<ParameterDescribe> {
        &mut self.parameters
    }
    /// Mutably access the result columns collection.
    pub const fn result_columns_mut(&mut self) -> &mut Collection<ResultSetColumn> {
        &mut self.result_columns
    }
}
