//! Context-free SQL syntax shared by the parser frontends.
//!
//! Names in this module are spelling, not catalog bindings. In particular, type
//! names and function names do not imply that an object exists. Recursive edges
//! always stay in this representation, including expressions inside query clauses.
//! Converters consume newly built child nodes to avoid cloning recursive trees;
//! ordinary semantic factories borrow their inputs.

// Syntax retention intentionally precedes semantic support for some SQL clauses.
// Construction tests cover these fields even when resolution does not consume them.
#![allow(dead_code)]

mod document;
mod postgres;
mod resolve;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;

use self::document::{
    CmdType, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
    JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding, JsonExprOp, JsonFormat,
    JsonFormatType, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate, JsonOutput,
    JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning, JsonScalar, JsonSerialize,
    JsonTable, JsonTableColumnType, JsonValue, JsonValueType, JsonWrapper, Merge, MergeMatchKind,
    MergeWhen, MultiAssignment, OverridingKind, Xml, XmlColumn, XmlExprOp, XmlOptionType,
    XmlSerialize, XmlTable,
};

use crate::StringSpan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dialect {
    Postgres,
    Sqlite,
}

impl Dialect {
    pub(crate) const fn postgres() -> Self {
        Self::Postgres
    }
    pub(crate) const fn sqlite() -> Self {
        Self::Sqlite
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Name(Vec<String>);

impl Name {
    fn from_parts(parts: &[String]) -> Self {
        Self(parts.to_vec())
    }

    fn local(value: &str) -> Self {
        Self(vec![value.to_owned()])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TypeName {
    name: Name,
    modifiers: Vec<Expr>,
    array_bounds: Vec<Option<i32>>,
    set_of: bool,
    percent_type: bool,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Null,
    Integer(i32),
    Numeric(String),
    String(String),
    Boolean(bool),
    BitString(String),
    Blob(String),
    Keyword(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Literal(Literal),
    Column(Name),
    Star(Name),
    Parameter(Parameter),
    Operator(Box<OperatorExpr>),
    Boolean(Box<BooleanExpr>),
    NullTest(Box<NullTest>),
    TruthTest(Box<TruthTest>),
    Cast(Box<Cast>),
    Collate(Box<Collate>),
    Call(Box<Call>),
    Case(Box<Case>),
    Subquery(Box<Subquery>),
    Array(Vec<Self>),
    Row(Vec<Self>),
    List(Vec<Self>),
    Parenthesized(Vec<Self>),
    Indirection(Box<Indirection>),
    NamedArgument(Box<NamedArgument>),
    SqlValue(SqlValue),
    Default,
    Raise(Box<Raise>),
    InTable(Box<InTable>),
    Document(document::DocumentExpr),
    Grouping(Vec<Self>),
    CurrentOf(CurrentOf),
    MergeAction,
}

#[derive(Clone, Debug)]
pub(crate) struct CurrentOf {
    name: String,
    parameter: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    position: u32,
    name: Option<String>,
    type_name: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct InTable {
    argument: Expr,
    negated: bool,
    table: Name,
    arguments: Vec<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorExpr {
    name: Name,
    kind: OperatorKind,
    left: Option<Expr>,
    right: Option<Expr>,
    escape: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OperatorKind {
    Ordinary,
    Any,
    All,
    Distinct,
    NotDistinct,
    NullIf,
    In,
    NotIn,
    Like,
    NotLike,
    ILike,
    Similar,
    Between,
    NotBetween,
    BetweenSymmetric,
    NotBetweenSymmetric,
}

#[derive(Clone, Debug)]
pub(crate) struct BooleanExpr {
    operator: BooleanOperator,
    arguments: Vec<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BooleanOperator {
    And,
    Or,
    Not,
}

#[derive(Clone, Debug)]
pub(crate) struct NullTest {
    argument: Expr,
    negated: bool,
    row: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TruthTest {
    argument: Expr,
    value: Option<bool>,
    negated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Cast {
    argument: Expr,
    target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct Collate {
    argument: Expr,
    name: Name,
}

#[derive(Clone, Debug)]
pub(crate) struct Call {
    kind: CallKind,
    name: Name,
    arguments: Vec<Expr>,
    star: bool,
    distinct: bool,
    variadic: bool,
    within_group: bool,
    within_order: Vec<Order>,
    order: Vec<Order>,
    filter: Option<Expr>,
    over: Option<Window>,
    location: Option<StringSpan>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CallKind {
    Ordinary,
    Coalesce,
    MinMax,
}

impl CallKind {
    const fn ordinary() -> Self {
        Self::Ordinary
    }
    fn special(name: &str) -> Self {
        if name == "coalesce" {
            Self::Coalesce
        } else {
            Self::MinMax
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Case {
    argument: Option<Expr>,
    arms: Vec<CaseArm>,
    otherwise: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct CaseArm {
    condition: Expr,
    result: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct Subquery {
    query: Query,
    kind: SubqueryKind,
    test: Option<Expr>,
    operator: Name,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SubqueryKind {
    Scalar,
    Exists,
    Any,
    All,
    Row,
    Array,
    MultiExpression,
}

#[derive(Clone, Debug)]
pub(crate) struct Indirection {
    argument: Expr,
    items: Vec<Access>,
}

#[derive(Clone, Debug)]
pub(crate) enum Access {
    Field(String),
    Star,
    Index(Expr),
    Slice(Slice),
}

#[derive(Clone, Debug)]
pub(crate) struct Slice {
    lower: Option<Expr>,
    upper: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct NamedArgument {
    name: String,
    argument: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct SqlValue {
    name: String,
    precision: Option<i32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Raise {
    action: RaiseAction,
    message: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RaiseAction {
    Rollback,
    Abort,
    Fail,
    Ignore,
    Replace,
}

#[derive(Clone, Debug)]
pub(crate) struct Query {
    with: With,
    body: QueryBody,
    order: Vec<Order>,
    limit: Option<Expr>,
    offset: Option<Expr>,
    with_ties: bool,
    locks: Vec<Lock>,
}

#[derive(Clone, Debug)]
pub(crate) enum QueryBody {
    Select(Box<Select>),
    Values(Vec<Vec<Expr>>),
    Set(Box<SetQuery>),
}

#[derive(Clone, Debug)]
pub(crate) struct SetQuery {
    operation: SetOperation,
    all: bool,
    left: Query,
    right: Query,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SetOperation {
    Union,
    Intersect,
    Except,
}

#[derive(Clone, Debug)]
pub(crate) struct Select {
    into: Option<Into>,
    distinct: Option<Vec<Expr>>,
    targets: Vec<Target>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    group: Vec<Grouping>,
    group_distinct: bool,
    having: Option<Expr>,
    windows: Vec<Window>,
}

#[derive(Clone, Debug)]
pub(crate) struct Target {
    expression: Expr,
    alias: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum Grouping {
    Expression(Expr),
    Empty,
    Rollup(Vec<Self>),
    Cube(Vec<Self>),
    Sets(Vec<Self>),
    Simple(Vec<Self>),
}

#[derive(Clone, Debug)]
pub(crate) struct Order {
    expression: Expr,
    descending: Option<bool>,
    nulls_first: Option<bool>,
    using: Option<Name>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct With {
    recursive: bool,
    entries: Vec<Cte>,
}

#[derive(Clone, Debug)]
pub(crate) struct Cte {
    name: String,
    columns: Vec<CteColumn>,
    materialized: Option<bool>,
    body: CteBody,
    search: Option<Search>,
    cycle: Option<Cycle>,
}

#[derive(Clone, Debug)]
pub(crate) struct CteColumn {
    name: String,
    collation: Option<String>,
    descending: Option<bool>,
}

impl Cte {
    fn column_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    }
}

impl CteColumn {
    fn from_names(names: &[String]) -> Vec<Self> {
        let mut columns = Vec::new();
        for name in names {
            columns.push(Self {
                name: name.clone(),
                collation: None,
                descending: None,
            });
        }
        columns
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CteBody {
    Query(Box<Query>),
    Insert(Box<Insert>),
    Update(Box<Update>),
    Delete(Box<Delete>),
    Merge(Box<Merge>),
}

#[derive(Clone, Debug)]
pub(crate) struct Search {
    columns: Vec<String>,
    breadth_first: bool,
    sequence: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Cycle {
    columns: Vec<String>,
    mark: String,
    value: Option<Expr>,
    default: Option<Expr>,
    path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Alias {
    name: String,
    columns: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum FromItem {
    Relation(Relation),
    Query(Box<DerivedTable>),
    Join(Box<Join>),
    Functions(TableFunctions),
    Group(Box<FromGroup>),
    Sample(Box<TableSample>),
    Xml(Box<XmlTable>),
    Json(Box<JsonTable>),
}

#[derive(Clone, Debug)]
pub(crate) struct Relation {
    name: Name,
    alias: Option<Alias>,
    inherit: bool,
    persistence: String,
    indexed: Option<String>,
    not_indexed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DerivedTable {
    query: Query,
    alias: Option<Alias>,
    lateral: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FromGroup {
    items: Vec<FromItem>,
    alias: Option<Alias>,
}

#[derive(Clone, Debug)]
pub(crate) struct Join {
    left: FromItem,
    right: FromItem,
    kind: JoinKind,
    natural: bool,
    condition: Option<Expr>,
    using: Vec<String>,
    using_alias: Option<Alias>,
    alias: Option<Alias>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

#[derive(Clone, Debug)]
pub(crate) struct TableFunctions {
    calls: Vec<TableFunction>,
    alias: Option<Alias>,
    lateral: bool,
    ordinality: bool,
    rows_from: bool,
    columns: Vec<TypedColumn>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableFunction {
    expression: Expr,
    columns: Vec<TypedColumn>,
}

#[derive(Clone, Debug)]
pub(crate) struct TypedColumn {
    name: String,
    type_name: TypeName,
    collation: Option<Name>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableSample {
    relation: FromItem,
    method: Name,
    arguments: Vec<Expr>,
    repeatable: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Window {
    name: Option<String>,
    reference: Option<String>,
    partition: Vec<Expr>,
    order: Vec<Order>,
    frame: Option<Frame>,
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    mode: FrameMode,
    start: FrameBound,
    end: Option<FrameBound>,
    exclusion: Option<FrameExclusion>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameMode {
    Rows,
    Range,
    Groups,
}

#[derive(Clone, Debug)]
pub(crate) enum FrameBound {
    UnboundedPreceding,
    Preceding(Expr),
    CurrentRow,
    Following(Expr),
    UnboundedFollowing,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameExclusion {
    CurrentRow,
    Group,
    Ties,
    NoOthers,
}

#[derive(Clone, Debug)]
pub(crate) struct Lock {
    relations: Vec<Name>,
    strength: Option<LockStrength>,
    wait: Option<LockWait>,
}

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    name: String,
    indirection: Vec<Access>,
    value: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Insert {
    with: With,
    relation: Relation,
    columns: Vec<Assignment>,
    source: Option<Query>,
    overriding: Option<OverridingKind>,
    conflict: Option<Conflict>,
    returning: Vec<Target>,
}

#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    action: Option<ConflictAction>,
    inference: Vec<IndexExpression>,
    inference_predicate: Option<Expr>,
    constraint: Option<String>,
    assignments: Vec<Assignment>,
    predicate: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexExpression {
    name: Option<String>,
    expression: Option<Expr>,
    collation: Name,
    operator_class: Name,
    options: Vec<SyntaxOption>,
    descending: Option<bool>,
    nulls_first: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct Into {
    relation: Relation,
    columns: Vec<String>,
    access_method: Option<String>,
    options: Vec<SyntaxOption>,
    on_commit: Option<CommitAction>,
    tablespace: Option<String>,
    skip_data: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SyntaxOption {
    namespace: Option<String>,
    name: String,
    value: Option<SyntaxValue>,
}

#[derive(Clone, Debug)]
pub(crate) enum SyntaxValue {
    Expression(Expr),
    Type(TypeName),
}

#[derive(Clone, Debug)]
pub(crate) struct Update {
    with: With,
    relation: Relation,
    assignments: Vec<Assignment>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
}

#[derive(Clone, Debug)]
pub(crate) struct Delete {
    with: With,
    relation: Relation,
    using: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LockStrength {
    None,
    KeyShare,
    Share,
    NoKeyUpdate,
    Update,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LockWait {
    Block,
    Skip,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConflictAction {
    None,
    Nothing,
    Update,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CommitAction {
    None,
    PreserveRows,
    DeleteRows,
    Drop,
}

impl BooleanOperator {
    const fn and() -> Self {
        Self::And
    }
    const fn or() -> Self {
        Self::Or
    }
    const fn not() -> Self {
        Self::Not
    }
}

impl OperatorKind {
    const fn ordinary() -> Self {
        Self::Ordinary
    }
    const fn not_between() -> Self {
        Self::NotBetween
    }
    const fn between() -> Self {
        Self::Between
    }
    const fn not_in() -> Self {
        Self::NotIn
    }
    const fn in_list() -> Self {
        Self::In
    }
    const fn not_like() -> Self {
        Self::NotLike
    }
    const fn like() -> Self {
        Self::Like
    }
}

impl SubqueryKind {
    const fn scalar() -> Self {
        Self::Scalar
    }
    const fn exists() -> Self {
        Self::Exists
    }
    const fn any() -> Self {
        Self::Any
    }
}
