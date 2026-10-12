mod deparse;

#[path = "postgres/document.rs"]
mod postgres;
#[path = "resolve/document.rs"]
mod resolve;
#[path = "document_validation.rs"]
mod validation;

use crate::StringSpan;
use crate::ast::{
    Alias, Assignment, Expr, FromItem, Name, Order, Query, Relation, Target, TypeName, Window, With,
};

#[derive(Clone, Debug)]
pub(crate) enum DocumentExpr {
    JsonFunction(Box<JsonFunction>),
    JsonPredicate(Box<JsonPredicate>),
    JsonParse(Box<JsonParse>),
    JsonScalar(Box<JsonScalar>),
    JsonSerialize(Box<JsonSerialize>),
    JsonObject(Box<JsonObject>),
    JsonArray(Box<JsonArray>),
    JsonArrayQuery(Box<JsonArrayQuery>),
    JsonObjectAggregate(Box<JsonObjectAggregate>),
    JsonArrayAggregate(Box<JsonArrayAggregate>),
    Xml(Box<Xml>),
    XmlSerialize(Box<XmlSerialize>),
    MultiAssignment(Box<MultiAssignment>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonExprOp {
    Exists,
    Query,
    Value,
    Table,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonFormatType {
    Default,
    Json,
    Jsonb,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonEncoding {
    Default,
    Utf8,
    Utf16,
    Utf32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonValueType {
    Any,
    Object,
    Array,
    Scalar,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonBehaviorType {
    Null,
    Error,
    Empty,
    True,
    False,
    Unknown,
    EmptyArray,
    EmptyObject,
    Default,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonWrapper {
    Unspec,
    None,
    Conditional,
    Unconditional,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonQuotes {
    Unspec,
    Keep,
    Omit,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JsonTableColumnType {
    ForOrdinality,
    Regular,
    Exists,
    Formatted,
    Nested,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum XmlExprOp {
    Xmlconcat,
    Xmlelement,
    Xmlforest,
    Xmlparse,
    Xmlpi,
    Xmlroot,
    Xmlserialize,
    Document,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum XmlOptionType {
    Document,
    Content,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum MergeMatchKind {
    Matched,
    NotMatchedBySource,
    NotMatchedByTarget,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CmdType {
    Unknown,
    Select,
    Update,
    Insert,
    Delete,
    Merge,
    Utility,
    Nothing,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OverridingKind {
    NotSet,
    UserValue,
    SystemValue,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonFormat {
    format_type: JsonFormatType,
    encoding: JsonEncoding,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonOutput {
    type_name: Option<TypeName>,
    returning: Option<JsonReturning>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonReturning {
    format: Option<JsonFormat>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonValue {
    raw_expr: Expr,
    format: Option<JsonFormat>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArgument {
    val: JsonValue,
    name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonBehavior {
    btype: JsonBehaviorType,
    expr: Option<Expr>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonPath {
    string: Expr,
    name: String,
    name_location: Option<StringSpan>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonKeyValue {
    key: Expr,
    value: JsonValue,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonFunction {
    op: JsonExprOp,
    column_name: String,
    context_item: JsonValue,
    pathspec: Expr,
    passing: Vec<JsonArgument>,
    output: Option<JsonOutput>,
    on_empty: Option<JsonBehavior>,
    on_error: Option<JsonBehavior>,
    wrapper: JsonWrapper,
    quotes: JsonQuotes,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonPredicate {
    expr: Expr,
    format: Option<JsonFormat>,
    item_type: JsonValueType,
    unique_keys: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonParse {
    expr: JsonValue,
    output: Option<JsonOutput>,
    unique_keys: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonScalar {
    expr: Expr,
    output: Option<JsonOutput>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonSerialize {
    expr: JsonValue,
    output: Option<JsonOutput>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonObject {
    exprs: Vec<JsonKeyValue>,
    output: Option<JsonOutput>,
    absent_on_null: bool,
    unique: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArray {
    exprs: Vec<JsonValue>,
    output: Option<JsonOutput>,
    absent_on_null: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArrayQuery {
    query: Query,
    output: Option<JsonOutput>,
    format: Option<JsonFormat>,
    absent_on_null: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonAggregate {
    output: Option<JsonOutput>,
    agg_filter: Option<Expr>,
    agg_order: Vec<Order>,
    over: Option<Window>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonObjectAggregate {
    constructor: JsonAggregate,
    arg: JsonKeyValue,
    absent_on_null: bool,
    unique: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArrayAggregate {
    constructor: JsonAggregate,
    arg: JsonValue,
    absent_on_null: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonTable {
    context_item: JsonValue,
    pathspec: JsonPath,
    passing: Vec<JsonArgument>,
    columns: Vec<JsonColumn>,
    on_error: Option<JsonBehavior>,
    alias: Option<Alias>,
    lateral: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonColumn {
    coltype: JsonTableColumnType,
    name: String,
    type_name: Option<TypeName>,
    pathspec: Option<JsonPath>,
    format: Option<JsonFormat>,
    wrapper: JsonWrapper,
    quotes: JsonQuotes,
    columns: Vec<Self>,
    on_empty: Option<JsonBehavior>,
    on_error: Option<JsonBehavior>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct Xml {
    op: XmlExprOp,
    name: String,
    named_args: Vec<Target>,
    arg_names: Name,
    args: Vec<Expr>,
    xmloption: XmlOptionType,
    indent: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlSerialize {
    xmloption: XmlOptionType,
    expr: Expr,
    type_name: TypeName,
    indent: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlTable {
    lateral: bool,
    docexpr: Option<Expr>,
    rowexpr: Expr,
    namespaces: Vec<Target>,
    columns: Vec<XmlColumn>,
    alias: Option<Alias>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlColumn {
    colname: String,
    type_name: Option<TypeName>,
    for_ordinality: bool,
    is_not_null: bool,
    colexpr: Option<Expr>,
    coldefexpr: Option<Expr>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct MultiAssignment {
    source: Expr,
    colno: i32,
    ncolumns: i32,
}

impl MultiAssignment {
    fn new(source: &Expr, colno: i32, ncolumns: i32) -> Self {
        Self {
            source: source.clone(),
            colno,
            ncolumns,
        }
    }
}

impl DocumentExpr {
    fn multi_assignment(source: &Expr, colno: i32, ncolumns: i32) -> Self {
        Self::MultiAssignment(Box::new(MultiAssignment::new(source, colno, ncolumns)))
    }
}

impl Expr {
    pub(in crate::ast) fn multi_assignment(source: &Self, colno: i32, ncolumns: i32) -> Self {
        Self::Document(DocumentExpr::multi_assignment(source, colno, ncolumns))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Merge {
    relation: Relation,
    source_relation: FromItem,
    join_condition: Expr,
    merge_when_clauses: Vec<MergeWhen>,
    returning_list: Vec<Target>,
    with_clause: With,
}

impl Merge {
    pub(super) const fn with_node(&self) -> &With {
        &self.with_clause
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MergeWhen {
    match_kind: MergeMatchKind,
    command_type: CmdType,
    overriding: OverridingKind,
    condition: Option<Expr>,
    target_list: Vec<Assignment>,
    values: Vec<Expr>,
}

impl Expr {
    pub(in crate::ast) fn multi_assignment_data(&self) -> Option<(&Self, i32, i32)> {
        self.document_multi_assignment()
    }
    fn document_multi_assignment(&self) -> Option<(&Self, i32, i32)> {
        match self {
            Self::Document(document) => document.multi_assignment_data(),
            _ => None,
        }
    }

    #[cfg(feature = "sqlite")]
    pub(in crate::ast) fn multi_assignment_data_mut(&mut self) -> Option<(&mut Self, i32, i32)> {
        match self {
            Self::Document(document) => document.multi_assignment_data_mut(),
            _ => None,
        }
    }
}
impl DocumentExpr {
    fn multi_assignment_data(&self) -> Option<(&Expr, i32, i32)> {
        match self {
            Self::MultiAssignment(value) => Some((&value.source, value.colno, value.ncolumns)),
            _ => None,
        }
    }

    #[cfg(feature = "sqlite")]
    pub(in crate::ast) fn multi_assignment_data_mut(&mut self) -> Option<(&mut Expr, i32, i32)> {
        match self {
            Self::MultiAssignment(value) => Some((&mut value.source, value.colno, value.ncolumns)),
            _ => None,
        }
    }
}
