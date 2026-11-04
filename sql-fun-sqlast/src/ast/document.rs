mod deparse;

mod projection_walk;

#[path = "postgres/document.rs"]
mod postgres;

#[path = "document_validation.rs"]
mod validation;

use crate::ast::ReturningOption;

use crate::{
    StringSpan,
    ast::{
        Alias, Assignment, Expr, FromItem, Name, Order, Query, Relation, Target, TypeName, Window,
        With,
    },
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
    pub(crate) format_type: JsonFormatType,
    pub(crate) encoding: JsonEncoding,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonOutput {
    pub(crate) type_name: Option<TypeName>,
    pub(crate) returning: Option<JsonReturning>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonReturning {
    pub(crate) format: Option<JsonFormat>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonValue {
    pub(crate) raw_expr: Expr,
    pub(crate) format: Option<JsonFormat>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArgument {
    pub(crate) val: JsonValue,
    pub(crate) name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonBehavior {
    pub(crate) btype: JsonBehaviorType,
    pub(crate) expr: Option<Expr>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonPath {
    pub(crate) string: Expr,
    pub(crate) name: String,
    pub(crate) name_location: Option<StringSpan>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonKeyValue {
    pub(crate) key: Expr,
    pub(crate) value: JsonValue,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonFunction {
    pub(crate) op: JsonExprOp,
    pub(crate) column_name: String,
    pub(crate) context_item: JsonValue,
    pub(crate) pathspec: Expr,
    pub(crate) passing: Vec<JsonArgument>,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) on_empty: Option<JsonBehavior>,
    pub(crate) on_error: Option<JsonBehavior>,
    pub(crate) wrapper: JsonWrapper,
    pub(crate) quotes: JsonQuotes,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonPredicate {
    pub(crate) expr: Expr,
    pub(crate) format: Option<JsonFormat>,
    pub(crate) item_type: JsonValueType,
    pub(crate) unique_keys: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonParse {
    pub(crate) expr: JsonValue,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) unique_keys: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonScalar {
    pub(crate) expr: Expr,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonSerialize {
    pub(crate) expr: JsonValue,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonObject {
    pub(crate) exprs: Vec<JsonKeyValue>,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) absent_on_null: bool,
    pub(crate) unique: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArray {
    pub(crate) exprs: Vec<JsonValue>,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) absent_on_null: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArrayQuery {
    pub(crate) query: Query,
    pub(crate) output: Option<JsonOutput>,
    pub(crate) format: Option<JsonFormat>,
    pub(crate) absent_on_null: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonAggregate {
    pub(crate) output: Option<JsonOutput>,
    pub(crate) agg_filter: Option<Expr>,
    pub(crate) agg_order: Vec<Order>,
    pub(crate) over: Option<Window>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonObjectAggregate {
    pub(crate) constructor: JsonAggregate,
    pub(crate) arg: JsonKeyValue,
    pub(crate) absent_on_null: bool,
    pub(crate) unique: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonArrayAggregate {
    pub(crate) constructor: JsonAggregate,
    pub(crate) arg: JsonValue,
    pub(crate) absent_on_null: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonTable {
    pub(crate) context_item: JsonValue,
    pub(crate) pathspec: JsonPath,
    pub(crate) passing: Vec<JsonArgument>,
    pub(crate) columns: Vec<JsonColumn>,
    pub(crate) on_error: Option<JsonBehavior>,
    pub(crate) alias: Option<Alias>,
    pub(crate) lateral: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsonColumn {
    pub(crate) coltype: JsonTableColumnType,
    pub(crate) name: String,
    pub(crate) type_name: Option<TypeName>,
    pub(crate) pathspec: Option<JsonPath>,
    pub(crate) format: Option<JsonFormat>,
    pub(crate) wrapper: JsonWrapper,
    pub(crate) quotes: JsonQuotes,
    pub(crate) columns: Vec<Self>,
    pub(crate) on_empty: Option<JsonBehavior>,
    pub(crate) on_error: Option<JsonBehavior>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct Xml {
    pub(crate) op: XmlExprOp,
    pub(crate) name: String,
    pub(crate) named_args: Vec<Target>,
    pub(crate) arg_names: Name,
    pub(crate) args: Vec<Expr>,
    pub(crate) xmloption: XmlOptionType,
    pub(crate) indent: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlSerialize {
    pub(crate) xmloption: XmlOptionType,
    pub(crate) expr: Expr,
    pub(crate) type_name: TypeName,
    pub(crate) indent: bool,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlTable {
    pub(crate) lateral: bool,
    pub(crate) docexpr: Option<Expr>,
    pub(crate) rowexpr: Expr,
    pub(crate) namespaces: Vec<Target>,
    pub(crate) columns: Vec<XmlColumn>,
    pub(crate) alias: Option<Alias>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct XmlColumn {
    pub(crate) colname: String,
    pub(crate) type_name: Option<TypeName>,
    pub(crate) for_ordinality: bool,
    pub(crate) is_not_null: bool,
    pub(crate) colexpr: Option<Expr>,
    pub(crate) coldefexpr: Option<Expr>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct MultiAssignment {
    pub(crate) source: Expr,
    pub(crate) colno: i32,
    pub(crate) ncolumns: i32,
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
    pub(crate) relation: Relation,
    pub(crate) source_relation: FromItem,
    pub(crate) join_condition: Expr,
    pub(crate) merge_when_clauses: Vec<MergeWhen>,
    pub(crate) returning_list: Vec<Target>,
    pub(crate) returning_options: Vec<ReturningOption>,
    pub(crate) with_clause: With,
}

impl Merge {
    pub(super) const fn with_node(&self) -> &With {
        &self.with_clause
    }

    pub(super) const fn with_node_mut(&mut self) -> &mut With {
        &mut self.with_clause
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MergeWhen {
    pub(crate) match_kind: MergeMatchKind,
    pub(crate) command_type: CmdType,
    pub(crate) overriding: OverridingKind,
    pub(crate) condition: Option<Expr>,
    pub(crate) target_list: Vec<Assignment>,
    pub(crate) values: Vec<Expr>,
}

impl Expr {
    pub(crate) fn multi_assignment_data(&self) -> Option<(&Self, i32, i32)> {
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
