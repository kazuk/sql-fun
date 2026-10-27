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
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalyzeQueryArgs {
    /// specify semantic analysis context
    context_handle: ContextHandle,
    /// Required, authoritative application SQL input.
    query: AnalyzeQueryInput,
    /// Optional resolved CTE dependencies, without application SQL.
    #[serde(default)]
    resolved_cte_catalog: Option<::sql_fun_core::ResolvedCteCatalog>,
}

impl AnalyzeQueryArgs {
    /// Submit explicit SQL without Rust extraction or a CTE dependency catalog.
    #[must_use]
    pub fn new(context_handle: &ContextHandle, query: &str) -> Self {
        Self {
            context_handle: context_handle.clone(),
            query: AnalyzeQueryInput::from_text(query),
            resolved_cte_catalog: None,
        }
    }

    /// Submit explicit inline application SQL with its resolved CTE dependencies.
    /// Takes ownership to preserve the recovered catalog without copying it.
    #[must_use]
    pub fn with_catalog(
        context_handle: &ContextHandle,
        query: &str,
        catalog: ::sql_fun_core::ResolvedCteCatalog,
    ) -> Self {
        Self {
            context_handle: context_handle.clone(),
            query: AnalyzeQueryInput::from_text(query),
            resolved_cte_catalog: Some(catalog),
        }
    }
}

#[cfg(test)]
mod test_catalog_request_transport {
    use super::AnalyzeQueryArgs;

    #[rstest::rstest]
    #[case::omitted(false)]
    #[case::explicit_null(true)]
    fn plain_query_remains_valid(#[case] explicit_null: bool) {
        let mut value = ::serde_json::json!({
            "context_handle":"context-1", "query":{"Text":"SELECT 1"}
        });
        if explicit_null {
            value["resolved_cte_catalog"] = ::serde_json::Value::Null;
        }
        let request: AnalyzeQueryArgs = ::serde_json::from_value(value).unwrap();
        assert!(request.resolved_cte_catalog().is_none());
        assert_eq!(request.query().text(), Some("SELECT 1"));
    }

    #[rstest::rstest]
    #[case::missing_query(r#"{"context_handle":"context-1"}"#)]
    #[case::null_query(r#"{"context_handle":"context-1","query":null}"#)]
    #[case::catalog_only(r#"{"context_handle":"context-1","resolved_cte_catalog":{"parameters":{"parameters":{}},"cte_queries":{}}}"#)]
    #[case::invalid_query(r#"{"context_handle":"context-1","query":42}"#)]
    #[case::invalid_catalog(
        r#"{"context_handle":"context-1","query":{"Text":"SELECT 1"},"resolved_cte_catalog":42}"#
    )]
    fn rejects_invalid_request_shape(#[case] input: &str) {
        assert!(::serde_json::from_str::<AnalyzeQueryArgs>(input).is_err());
    }

    #[test]
    fn constructor_preserves_dependency_order_aliases_and_binding_metadata() {
        use ::sql_fun_core::{
            CteName, CteParameterCollection, PlaceholderedSql, ResolvedCteCatalog,
        };
        let base = PlaceholderedSql::from_query("SELECT ${shared:int4} FROM a_visible");
        let leaf = PlaceholderedSql::from_query("SELECT ${shared:int4} AS id");
        let mut bindings = CteParameterCollection::default();
        for (name, hints) in leaf.parameters() {
            bindings.push_query_parameter(&CteName::new("library::leaf"), name, hints);
        }
        let mut catalog = ::serde_json::to_value(ResolvedCteCatalog::default()).unwrap();
        catalog["parameters"] = ::serde_json::to_value(bindings).unwrap();
        catalog["cte_queries"]["z_dependency"] = ::serde_json::to_value(&leaf).unwrap();
        catalog["cte_queries"]["a_visible"] =
            ::serde_json::to_value(PlaceholderedSql::from_query("SELECT id FROM z_dependency"))
                .unwrap();
        let handle = crate::payloads::ContextHandle::from("context-1".to_owned());
        let request = AnalyzeQueryArgs::with_catalog(
            &handle,
            &base.encode(),
            ::serde_json::from_value(catalog.clone()).unwrap(),
        );
        let wire = ::serde_json::to_value(request).unwrap();
        assert_eq!(wire["query"]["Text"], base.encode());
        assert!(wire["resolved_cte_catalog"].get("base_query").is_none());
        assert_eq!(wire["resolved_cte_catalog"], catalog);
        let restored: AnalyzeQueryArgs = ::serde_json::from_value(wire).unwrap();
        let restored = restored.resolved_cte_catalog().as_ref().unwrap();
        let aliases = restored
            .try_map_cte_queries(|alias, _| {
                Ok::<_, std::convert::Infallible>(alias.as_str().to_owned())
            })
            .unwrap();
        assert_eq!(aliases, ["z_dependency", "a_visible"]);
        let mut order = Vec::new();
        restored.append_cte_parameter_order(&mut order);
        assert_eq!(order[0].as_str(), "shared");
        assert_eq!(
            ::serde_json::to_value(restored.parameters()).unwrap(),
            catalog["parameters"]
        );
    }
}

/// response for `analyze_query`
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalyzeQueryResponse {
    queries: Collection<AnalyzeQueryItem>,
}

impl super::CollectionTraversal for AnalyzeQueryResponse {
    fn collections(&self) -> Vec<&dyn super::CollectionReference> {
        vec![&self.queries]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn super::CollectionReference> {
        vec![&mut self.queries]
    }
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
    /// merge statement
    Merge,
    /// Any other valid SQL statement, including DDL.
    Other,
}

/// analyzed single query
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct AnalyzeQueryItem {
    /// query span in input
    query_span: Option<TextSpan>,

    /// Executable SQL deparsed from the realized AST, when rendering succeeds.
    execute_sql: Option<String>,

    /// Whether analysis finished without errors or unresolved parameter types.
    complete: bool,

    /// query statement kind
    query_kind: Option<QueryKind>,

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

impl super::CollectionTraversal for AnalyzeQueryItem {
    fn collections(&self) -> Vec<&dyn super::CollectionReference> {
        vec![
            &self.probrems,
            &self.accessing_relations,
            &self.column_access,
            &self.parameters,
            &self.result_columns,
        ]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn super::CollectionReference> {
        vec![
            &mut self.probrems,
            &mut self.accessing_relations,
            &mut self.column_access,
            &mut self.parameters,
            &mut self.result_columns,
        ]
    }
}

/// parameter spec
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ParameterDescribe {
    /// name for parameter
    name: String,
    /// parameter usage in SQL text
    spans: Vec<TextSpan>,
    /// Actual one-based numbered SQL placeholder identity, independent of collection order.
    parameter_index: u32,
    /// Finalized SQL type, absent when inference has not established one.
    parameter_type: Option<TypeReference>,
    /// parameter nullability
    nullability: Nullability,
}

impl super::CollectionTraversal for ParameterDescribe {}

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

impl super::CollectionTraversal for ColumnAccess {}

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

impl super::CollectionTraversal for ResultSetColumn {}

impl AnalyzeQueryInput {
    /// Select inline SQL as the application query input.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self::Text(text.to_owned())
    }

    /// Inline SQL, if explicitly selected.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text)
        } else {
            None
        }
    }
    /// Local SQL file, if explicitly selected.
    #[must_use]
    pub fn file(&self) -> Option<&std::path::Path> {
        if let Self::File(path) = self {
            Some(path)
        } else {
            None
        }
    }
}

impl AnalyzeQueryResponse {
    /// Construct an ordered query result page.
    #[must_use]
    pub const fn new(queries: Collection<AnalyzeQueryItem>) -> Self {
        Self { queries }
    }
}

impl AnalyzeQueryItem {
    /// Construct an analysis result; unavailable fields are not fabricated.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the DTO constructor owns independent collection pages without an extra assembly model"
    )]
    pub const fn new(
        query_span: Option<TextSpan>,
        query_kind: Option<QueryKind>,
        execute_sql: Option<String>,
        complete: bool,
        probrems: Collection<AnalysisProbrem>,
        parameters: Collection<ParameterDescribe>,
        result_columns: Collection<ResultSetColumn>,
        accessing_relations: Collection<ObjectSummary>,
        column_access: Collection<ColumnAccess>,
    ) -> Self {
        Self {
            query_span,
            execute_sql,
            complete,
            query_kind,
            probrems,
            accessing_relations,
            column_access,
            parameters,
            result_columns,
        }
    }
}

impl ParameterDescribe {
    /// Describe an actual SQL binding, with an optional finalized type.
    #[must_use]
    pub fn new(
        name: &str,
        parameter_index: u32,
        parameter_type: Option<TypeReference>,
        spans: Vec<TextSpan>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            parameter_index,
            parameter_type,
            spans,
            nullability: Nullability::Unknown,
        }
    }
}

impl ResultSetColumn {
    /// Preserve SQLAST column metadata, including an unavailable type.
    #[must_use]
    pub fn new(
        ordinal: u32,
        name: &str,
        column_type: Option<TypeReference>,
        nullability: Nullability,
    ) -> Self {
        Self {
            ordinal,
            name: name.to_owned(),
            column_type,
            nullability,
        }
    }
}

#[cfg(test)]
mod test_analyze_query_contract {
    use super::{AnalyzeQueryArgs, ParameterDescribe, ResultSetColumn};
    use crate::payloads::{Nullability, TypeReference};

    #[test]
    fn resolved_catalog_round_trips_through_existing_core_type() {
        let catalog = ::sql_fun_core::ResolvedCteCatalog::default();
        let value = ::serde_json::json!({"context_handle":"context-1", "query":{"Text":"SELECT ${id:int4}"}, "resolved_cte_catalog":catalog});
        let args: AnalyzeQueryArgs = ::serde_json::from_value(value.clone()).unwrap();
        assert_eq!(::serde_json::to_value(args).unwrap(), value);
    }

    #[test]
    fn finalized_binding_identity_and_unknown_result_type_survive_json() {
        let parameter = ParameterDescribe::new(
            "third",
            3,
            Some(TypeReference::new("pg_catalog", "int4", false)),
            Vec::new(),
        );
        let json = ::serde_json::to_value(parameter).unwrap();
        assert_eq!(json["parameter_index"], 3);
        assert_eq!(json["parameter_type"]["local_name"], "int4");
        assert!(json.get("type_candidate").is_none());
        let column = ResultSetColumn::new(0, "unknown", None, Nullability::from_not_null(None));
        let json = ::serde_json::to_value(column).unwrap();
        let column: ResultSetColumn = ::serde_json::from_value(json.clone()).unwrap();
        assert_eq!(::serde_json::to_value(column).unwrap(), json);
        assert!(json["column_type"].is_null());
    }

    #[cfg(feature = "openrpc-schema")]
    #[test]
    fn schema_includes_real_binding_index_and_finalized_type() {
        let schema = ::serde_json::to_value(::schemars::schema_for!(ParameterDescribe)).unwrap();
        assert!(schema["properties"].get("parameter_index").is_some());
        assert!(schema["properties"].get("parameter_type").is_some());
        assert!(schema["properties"].get("type_candidate").is_none());
        let schema = ::serde_json::to_value(::schemars::schema_for!(super::QueryKind)).unwrap();
        let mut kinds = Vec::new();
        for variant in schema["oneOf"].as_array().unwrap() {
            kinds.push(variant["const"].as_str().unwrap());
        }
        for kind in ["Merge", "Truncate", "Other"] {
            assert!(kinds.contains(&kind));
        }
        let schema = ::serde_json::to_value(::schemars::schema_for!(AnalyzeQueryArgs)).unwrap();
        assert!(schema["properties"].get("resolved_cte_catalog").is_some());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&::serde_json::json!("query")));
        assert!(!required.contains(&::serde_json::json!("resolved_cte_catalog")));
        let catalog_schema =
            ::serde_json::to_value(::schemars::schema_for!(::sql_fun_core::ResolvedCteCatalog))
                .unwrap();
        assert!(catalog_schema["properties"].get("base_query").is_none());
    }
}
