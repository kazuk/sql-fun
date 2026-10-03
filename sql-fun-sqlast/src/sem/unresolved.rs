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

use sql_fun_core::IVec;

use self::document::{
    CmdType, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
    JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding, JsonExprOp, JsonFormat,
    JsonFormatType, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate, JsonOutput,
    JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning, JsonScalar, JsonSerialize,
    JsonTableColumnType, JsonValue, JsonValueType, JsonWrapper, Merge, MergeMatchKind, MergeWhen,
    MultiAssignment, OverridingKind, Xml, XmlColumn, XmlExprOp, XmlOptionType, XmlSerialize,
};

pub(crate) use self::document::{JsonTable, XmlTable};

use crate::StringSpan;

/// A context-free top-level SQL statement.
///
/// This is deliberately private while statement-family coverage is incomplete.
/// Parser frontends construct it without catalog access; semantic resolution is
/// performed only after construction has completed.
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    Query(Query),
    Insert(Insert),
    Update(Update),
    Delete(Delete),
    View(View),
    Extension(Extension),
    VariableSet(VariableSet),
    EnumType(EnumType),
    Sequence(Sequence),
    DropTable(DropTable),
    AlterSequence(AlterSequence),
    Comment(Comment),
}

impl Statement {
    const fn query(value: Query) -> Self {
        Self::Query(value)
    }

    const fn insert(value: Insert) -> Self {
        Self::Insert(value)
    }

    const fn update(value: Update) -> Self {
        Self::Update(value)
    }

    const fn delete(value: Delete) -> Self {
        Self::Delete(value)
    }

    const fn view(value: View) -> Self {
        Self::View(value)
    }

    const fn extension(value: Extension) -> Self {
        Self::Extension(value)
    }

    const fn variable_set(value: VariableSet) -> Self {
        Self::VariableSet(value)
    }

    const fn enum_type(value: EnumType) -> Self {
        Self::EnumType(value)
    }

    const fn sequence(value: Sequence) -> Self {
        Self::Sequence(value)
    }

    const fn drop_table(value: DropTable) -> Self {
        Self::DropTable(value)
    }

    const fn alter_sequence(value: AlterSequence) -> Self {
        Self::AlterSequence(value)
    }

    const fn comment(value: Comment) -> Self {
        Self::Comment(value)
    }

    pub(crate) fn resolve(
        &self,
        catalog: &dyn crate::sem::resolution::Catalog,
        dialect: Dialect,
    ) -> crate::sem::resolution::ResolutionOutcome<crate::sem::SemAst> {
        match self {
            Self::Query(query) => query
                .resolve(catalog, dialect)
                .map(|query| crate::sem::SemAst::select(&query)),
            // DML semantic analysis currently has no catalog-dependent result.
            // Keep construction at the common boundary so that its eventual
            // resolver can replace these markers without changing frontends.
            Self::Insert(_) => crate::sem::resolution::Resolution::new(catalog).finish(Ok(
                crate::sem::SemAst::InsertStatement(crate::sem::InsertStatement::empty()),
            )),
            Self::Update(_) => crate::sem::resolution::Resolution::new(catalog).finish(Ok(
                crate::sem::SemAst::UpdateStatement(crate::sem::UpdateStatement::empty()),
            )),
            Self::Delete(_) => crate::sem::resolution::Resolution::new(catalog).finish(Ok(
                crate::sem::SemAst::DeleteStatement(crate::sem::DeleteStatement::empty()),
            )),
            Self::View(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "view requires catalog application",
                ),
            ),
            Self::Extension(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "extension requires catalog application",
                ),
            ),
            Self::VariableSet(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "variable set requires catalog application",
                ),
            ),
            Self::EnumType(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "enum type requires catalog application",
                ),
            ),
            Self::Sequence(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "sequence requires catalog application",
                ),
            ),
            Self::DropTable(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "drop table requires catalog application",
                ),
            ),
            Self::AlterSequence(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "alter sequence requires catalog application",
                ),
            ),
            Self::Comment(_) => crate::sem::resolution::Resolution::new(catalog).finish(
                crate::sem::AnalysisError::raise_unexpected_input(
                    "comment requires catalog application",
                ),
            ),
        }
    }

    pub(crate) fn resolve_and_apply<TContext>(
        &self,
        context: TContext,
        dialect: Dialect,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        match self {
            Self::View(view) => view.resolve_and_apply(context, dialect),
            Self::Extension(extension) => extension.apply(context),
            Self::VariableSet(variable_set) => variable_set.resolve_and_apply(context, dialect),
            Self::EnumType(enum_type) => enum_type.apply(context),
            Self::Sequence(sequence) => sequence.apply(context),
            Self::DropTable(drop_table) => drop_table.apply(context),
            Self::AlterSequence(alter_sequence) => alter_sequence.apply(context),
            Self::Comment(comment) => comment.apply(context),
            _ => {
                let outcome = self.resolve(&context, dialect);
                let mut context = context;
                let statement = outcome.report(&mut context)?;
                Ok(crate::AstAndContextPair::new(statement, context))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Comment {
    target: Vec<String>,
    object_kind: crate::sem::ObjectKind,
    text: String,
}

impl Comment {
    fn new(target: Vec<String>, object_kind: crate::sem::ObjectKind, text: &str) -> Self {
        Self {
            target,
            object_kind,
            text: text.to_owned(),
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let comment =
            crate::sem::Comment::new(&self.target.join("."), self.object_kind, &self.text);
        let context = context.apply_comment(&comment)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::Comment(comment),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterSequence {
    name: Name,
    options: Vec<SyntaxOption>,
    for_identity: bool,
    missing_ok: bool,
}

impl AlterSequence {
    fn new(name: Name, options: Vec<SyntaxOption>, for_identity: bool, missing_ok: bool) -> Self {
        Self {
            name,
            options,
            for_identity,
            missing_ok,
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let alter_sequence = crate::sem::AlterSequence::new(&self.name.full_name()?, &[]);
        let context = context.apply_alter_sequence(&alter_sequence)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::AlterSequence(alter_sequence),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DropTable {
    names: Vec<Name>,
    if_exists: bool,
    cascade: bool,
    concurrent: bool,
}

impl DropTable {
    fn new(names: Vec<Name>, if_exists: bool, cascade: bool, concurrent: bool) -> Self {
        Self {
            names,
            if_exists,
            cascade,
            concurrent,
        }
    }

    fn apply<TContext>(
        &self,
        mut context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let mut names = Vec::new();
        for name in &self.names {
            let table_name = crate::sem::TableName::from(name.full_name()?);
            if context.get_table(&table_name).is_none() {
                context
                    .report_problem(crate::sem::AnalysisProblem::table_not_found(&table_name))?;
            }
            names.push(table_name);
        }
        let drop_table = crate::sem::DropTable::new(&names, self.if_exists, self.cascade);
        context = context.apply_drop_table(&drop_table)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::DropTable(drop_table),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Sequence {
    name: Name,
    options: Vec<SyntaxOption>,
    owner_id: u32,
    for_identity: bool,
    if_not_exists: bool,
}

impl Sequence {
    fn new(
        name: Name,
        options: Vec<SyntaxOption>,
        owner_id: u32,
        for_identity: bool,
        if_not_exists: bool,
    ) -> Self {
        Self {
            name,
            options,
            owner_id,
            for_identity,
            if_not_exists,
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let sequence = crate::sem::CreateSequence::new(&self.name.full_name()?);
        let context = context.apply_create_sequence(&sequence)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateSequence(sequence),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EnumType {
    name: Name,
    values: Vec<String>,
}

impl EnumType {
    fn new(name: Name, values: Vec<String>) -> Self {
        Self { name, values }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let enum_type = crate::sem::CreateEnum::new(&self.name.full_name()?, &self.values);
        let context = context.apply_create_enum(&enum_type)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateEnum(enum_type),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VariableSet {
    name: String,
    arguments: Vec<Expr>,
}

impl VariableSet {
    fn new(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            name: name.to_owned(),
            arguments,
        }
    }

    fn resolve_and_apply<TContext>(
        &self,
        context: TContext,
        dialect: Dialect,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let [argument] = self.arguments.as_slice() else {
            return crate::sem::AnalysisError::raise_unexpected_input(
                "VariableSetStmt.args len!=1",
            );
        };
        let outcome = argument.resolve(
            &context,
            dialect,
            &crate::sem::ScalarAnalysisContext::default(),
        );
        let mut context = context;
        let value = outcome.report(&mut context)?;
        let variable_set = crate::sem::VariableSet::new(&self.name, &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::VariableSet(variable_set),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Extension {
    name: String,
}

impl Extension {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let extension = crate::sem::CreateExtension::from_name(&self.name);
        let context = context.apply_create_extension(&extension)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateExtension(extension),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct View {
    name: Name,
    aliases: Vec<String>,
    query: Query,
    replace: bool,
    options: Vec<SyntaxOption>,
    check_option: ViewCheckOption,
    temporary: bool,
    if_not_exists: bool,
    materialized: bool,
}

impl View {
    const fn new(
        name: Name,
        aliases: Vec<String>,
        query: Query,
        replace: bool,
        options: Vec<SyntaxOption>,
        check_option: ViewCheckOption,
        temporary: bool,
        if_not_exists: bool,
        materialized: bool,
    ) -> Self {
        Self {
            name,
            aliases,
            query,
            replace,
            options,
            check_option,
            temporary,
            if_not_exists,
            materialized,
        }
    }

    fn resolve_and_apply<TContext>(
        &self,
        context: TContext,
        dialect: Dialect,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let outcome = self.query.resolve(&context, dialect);
        let mut context = context;
        let query = outcome.report(&mut context)?;
        let view = crate::sem::CreateView::new(
            &self.name.full_name()?,
            &query,
            &[],
            &None,
            self.materialized,
        );
        let context = context.apply_create_view(&view)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateView(view),
            context,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ViewCheckOption {
    None,
    Local,
    Cascaded,
}

pub trait TryFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens(
        value: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error>;
}

pub trait TryFromWithTokensOpt<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens_opt(
        value: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error>;
}

pub trait TryFromOpt<T>: TryFrom<T> {
    fn try_from_opt(value: Option<T>) -> Result<Option<Self>, <Self as TryFrom<T>>::Error>
    where
        Self: Sized,
    {
        value.map(Self::try_from).transpose()
    }
}

impl<T, U> TryFromOpt<T> for U where U: TryFrom<T> {}

pub trait TryFromList<T>: Sized {
    type Error;

    fn try_from_list(values: &[T]) -> Result<Vec<Self>, Self::Error>;
}

impl<T, U, E> TryFromList<T> for U
where
    for<'a> U: TryFrom<&'a T, Error = E>,
{
    type Error = E;

    fn try_from_list(values: &[T]) -> Result<Vec<Self>, Self::Error> {
        values.iter().map(Self::try_from).collect()
    }
}

pub trait FromList<T>: Sized {
    fn from_list(values: &[T]) -> Vec<Self>;
}

impl<T, U> FromList<T> for U
where
    for<'a> U: From<&'a T>,
{
    fn from_list(values: &[T]) -> Vec<Self> {
        values.iter().map(Self::from).collect()
    }
}

pub trait FromOpt<T>: Sized {
    fn from_opt(value: Option<T>) -> Option<Self>;
}

impl<T, U> FromOpt<T> for U
where
    U: From<T>,
{
    fn from_opt(value: Option<T>) -> Option<Self> {
        value.map(Self::from)
    }
}

pub trait TryCollectFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_collect_from_with_tokens(
        nodes: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error>;
}

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

impl CteBody {
    pub(crate) fn query(query: Query) -> Self {
        Self::Query(Box::new(query))
    }

    pub(crate) fn insert(insert: Insert) -> Self {
        Self::Insert(Box::new(insert))
    }

    pub(crate) fn update(update: Update) -> Self {
        Self::Update(Box::new(update))
    }

    pub(crate) fn delete(delete: Delete) -> Self {
        Self::Delete(Box::new(delete))
    }

    pub(crate) fn merge(merge: Merge) -> Self {
        Self::Merge(Box::new(merge))
    }
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

impl FromItem {
    pub(crate) fn relation(relation: Relation) -> Self {
        Self::Relation(relation)
    }

    pub(crate) fn query(query: DerivedTable) -> Self {
        Self::Query(Box::new(query))
    }

    pub(crate) fn join(join: Join) -> Self {
        Self::Join(Box::new(join))
    }

    pub(crate) fn functions(functions: TableFunctions) -> Self {
        Self::Functions(functions)
    }

    pub(crate) fn group(group: FromGroup) -> Self {
        Self::Group(Box::new(group))
    }

    pub(crate) fn sample(sample: TableSample) -> Self {
        Self::Sample(Box::new(sample))
    }

    pub(crate) fn xml(table: XmlTable) -> Self {
        Self::Xml(Box::new(table))
    }

    pub(crate) fn json(table: JsonTable) -> Self {
        Self::Json(Box::new(table))
    }
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

impl TableFunctions {
    pub(crate) fn single(call: TableFunction, alias: Option<Alias>) -> Self {
        Self {
            calls: vec![call],
            alias,
            lateral: false,
            ordinality: false,
            rows_from: false,
            columns: Vec::new(),
        }
    }
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
