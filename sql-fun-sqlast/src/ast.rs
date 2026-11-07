//! Context-free SQL syntax shared by the parser frontends.
//!
//! Names in this module are spelling, not catalog bindings. In particular, type
//! names and function names do not imply that an object exists. Recursive edges
//! always stay in this representation, including expressions inside query clauses.
//! Converters consume newly built child nodes to avoid cloning recursive trees;
//! ordinary semantic factories borrow their inputs.

#![allow(dead_code)]

pub(crate) mod client_type;
pub(crate) mod ddl;
mod default_privileges;
pub(crate) mod deparse;
pub(crate) mod document;
mod drop_behavior;
mod foreign_table;
mod mutation;
mod options;
pub(crate) mod postgres;
pub(crate) mod projection;
mod public;
mod query_construction;
mod returning_options;
mod rows_from;
mod rule;
#[cfg(feature = "sqlite")]
mod sqlite;
mod traversal;
mod trigger;
pub(crate) mod validation;

pub(crate) use self::{
    ddl::{CompositeType, Domain, Function, Index, Schema, TableAs},
    deparse::render_full_name,
    document::{
        CmdType, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
        JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding, JsonExprOp, JsonFormat,
        JsonFormatType, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate, JsonOutput,
        JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning, JsonScalar, JsonSerialize,
        JsonTable, JsonTableColumnType, JsonValue, JsonValueType, JsonWrapper, Merge,
        MergeMatchKind, MergeWhen, MultiAssignment, OverridingKind, Xml, XmlColumn, XmlExprOp,
        XmlOptionType, XmlSerialize, XmlTable,
    },
};
pub use self::{
    default_privileges::{
        DefaultPrivilegeOption, DefaultPrivileges, PrivilegeGrantee, PrivilegeObject,
        PrivilegeSpecification,
    },
    deparse::DeparseError,
    foreign_table::{
        FdwOption, ForeignTable, ForeignTableElement, ForeignTableElementKind, TableLike,
    },
    mutation::{AstMutationError, CteDefinition, HaveWithClause, WithClauseMut},
    options::{
        ConflictClause, CreateTableMode, Deferrability, Deferrable, ForeignKeyAction,
        ForeignKeyMatchType, GeneratedColumnMode, IdentityColumnMode, ObjectKind, Validity,
    },
    public::{CommonTableExpression, Statement, StatementKind, WithClause},
};
use crate::{
    StringSpan,
    ast::projection::ProjectionError,
    syn::{self, Opt, ScanToken},
};
use sql_fun_core::IVec;

// Syntax retention intentionally precedes semantic support for some SQL clauses.
// Construction tests cover these fields even when resolution does not consume them.

#[derive(Clone, Debug, Default)]
pub(crate) struct TableExtras {
    unlogged: bool,
    inherits: Vec<Name>,
    of_type: Option<TypeName>,
    on_commit: Option<CommitAction>,
    tablespace: Option<String>,
    access_method: Option<String>,
    partition_spec: Option<PartitionSpec>,
    partition_bound: Option<PartitionBound>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionSpec {
    strategy: String,
    keys: Vec<IndexExpression>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionBound {
    strategy: String,
    is_default: bool,
    modulus: i32,
    remainder: i32,
    list: Vec<Expr>,
    lower: Vec<PartitionDatum>,
    upper: Vec<PartitionDatum>,
}
#[derive(Clone, Debug)]
pub(crate) enum PartitionDatum {
    Min,
    Max,
    Value(Expr),
}
#[derive(Clone, Debug, Default, derive_getters::Getters)]
pub(crate) struct ConstraintExtras {
    #[getter(copy)]
    enforced: Option<bool>,

    #[getter(copy)]
    without_overlaps: bool,

    #[getter(copy)]
    fk_with_period: bool,

    #[getter(copy)]
    pk_with_period: bool,

    #[getter(copy)]
    deferrable: bool,

    #[getter(copy)]
    initially_deferred: bool,
    #[getter(copy)]
    skip_validation: bool,
    #[getter(copy)]
    no_inherit: bool,
    #[getter(skip)]
    nulls_not_distinct: bool,
    #[getter(skip)]
    include: Vec<String>,
    #[getter(skip)]
    options: Vec<SyntaxOption>,
    #[getter(skip)]
    index_name: Option<String>,
    #[getter(skip)]
    tablespace: Option<String>,
    #[getter(skip)]
    access_method: Option<String>,
    #[getter(skip)]
    predicate: Option<Expr>,
    #[getter(skip)]
    exclusions: Vec<ExclusionElement>,

    delete_columns: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct ExclusionElement {
    expression: IndexExpression,
    operator: Name,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Table {
    #[getter(skip)]
    extras: TableExtras,
    name: Name,
    columns: Vec<Column>,
    constraints: Vec<TableConstraint>,
    #[getter(copy)]
    temporary: bool,
    #[getter(copy)]
    mode: CreateTableMode,
    query: Option<Query>,
    options: Vec<SyntaxOption>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Column {
    collation: Name,
    #[getter(skip)]
    compression: Option<String>,
    #[getter(skip)]
    storage: Option<String>,
    name: String,
    type_name: Option<TypeName>,
    #[getter(skip)]
    type_mod: Option<i32>,
    constraints: Vec<TableConstraint>,
    #[getter(copy)]
    not_null: Option<bool>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TableConstraint {
    extras: ConstraintExtras,
    name: Option<String>,
    keys: Vec<String>,
    constraint: ColumnConstraint,

    order: Vec<Order>,
    #[getter(skip)]
    auto_increment: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ColumnConstraint {
    Enforcement(bool),
    Nullability(bool, Option<ConflictClause>, Option<Validity>),
    Default(Expr),
    Check(Expr),
    PrimaryKey(Option<ConflictClause>, Option<Deferrable>),
    Unique(Option<ConflictClause>),
    Identity(IdentityColumnMode),
    Generated(Expr, Option<GeneratedColumnMode>),
    ForeignKey(ForeignReference),
    Exclusion,
    Deferrability(bool),
    Deferred,
    Immediate,
    Collate(String),
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct ForeignReference {
    table: Name,
    columns: Vec<String>,
    match_type: Option<ForeignKeyMatchType>,
    delete_action: Option<ForeignKeyAction>,
    update_action: Option<ForeignKeyAction>,
    deferrability: Option<Deferrability>,
    validity: Option<Validity>,
    ignored_matches: Vec<String>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct AlterRelation {
    name: Name,
    #[getter(copy)]
    kind: AlterRelationKind,
    commands: Vec<AlterCommand>,
    #[getter(copy)]
    if_exists: bool,
    #[getter(skip)]
    only: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AlterRelationKind {
    Table,
    ForeignTable,
    Index,
    CompositeType,
    View,
    MaterializedView,
    Sequence,
}

#[derive(Clone, Debug)]
pub(crate) enum AlterCommand {
    Postgres(Box<ddl::PostgresAlterCommand>),
    Owner(Role),
    Default(String, Expr),
    Constraint(TableConstraint),
    Cluster(String),
    Rename(Name),
    AddColumn(Column),
    AlterColumn(String, Column),
    RenameColumn(String, String),
    DropColumn(String),
    Unsupported(String),
}

/// A context-free top-level SQL statement.
///
/// This is deliberately private while statement-family coverage is incomplete.
/// Parser frontends construct it without catalog access; semantic resolution is
/// performed only after construction has completed.
#[derive(Clone, Debug)]
pub(crate) enum StatementNode {
    Query(Query),
    Insert(Insert),
    Update(Update),
    Delete(Delete),
    Merge(Merge),
    View(View),
    DropRule(DropRule),
    CreateTrigger(CreateTrigger),
    CreateRule(CreateRule),
    DropTrigger(DropTrigger),
    Extension(Extension),
    VariableSet(VariableSet),
    EnumType(EnumType),
    Sequence(Sequence),
    DropTable(DropTable),
    AlterSequence(AlterSequence),
    Comment(Comment),
    AlterOwner(AlterOwner),
    CompositeType(CompositeType),
    Domain(Domain),
    Function(Function),
    Schema(Schema),
    Index(Index),
    TableAs(TableAs),
    Table(Table),
    DefaultPrivileges(DefaultPrivileges),
    ForeignTable(ForeignTable),
    AlterRelation(AlterRelation),
}

impl StatementNode {
    const fn default_privileges(value: DefaultPrivileges) -> Self {
        Self::DefaultPrivileges(value)
    }
    const fn foreign_table(value: ForeignTable) -> Self {
        Self::ForeignTable(value)
    }

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

    const fn merge(value: Merge) -> Self {
        Self::Merge(value)
    }

    const fn create_trigger(value: CreateTrigger) -> Self {
        Self::CreateTrigger(value)
    }

    const fn create_rule(value: CreateRule) -> Self {
        Self::CreateRule(value)
    }

    const fn drop_trigger(value: DropTrigger) -> Self {
        Self::DropTrigger(value)
    }

    const fn drop_rule(value: DropRule) -> Self {
        Self::DropRule(value)
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

    const fn alter_owner(value: AlterOwner) -> Self {
        Self::AlterOwner(value)
    }

    const fn composite_type(value: CompositeType) -> Self {
        Self::CompositeType(value)
    }

    const fn alter_relation(value: AlterRelation) -> Self {
        Self::AlterRelation(value)
    }

    const fn domain(value: Domain) -> Self {
        Self::Domain(value)
    }

    const fn function(value: Function) -> Self {
        Self::Function(value)
    }
    const fn schema(value: Schema) -> Self {
        Self::Schema(value)
    }
    const fn index(value: Index) -> Self {
        Self::Index(value)
    }
    const fn table_as(value: TableAs) -> Self {
        Self::TableAs(value)
    }

    const fn table(value: Table) -> Self {
        Self::Table(value)
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct AlterOwner {
    #[getter(copy)]
    object_kind: Option<ObjectKind>,
    relation: Option<Name>,
    object: Option<ObjectTarget>,
    owner: Role,
}

/// A named database object, optionally identified by its argument type list.
/// `None` distinguishes an unspecified signature from an explicitly empty `()`.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct ObjectTarget {
    name: Vec<String>,
    argument_types: Option<Vec<ObjectArgument>>,
    #[getter(skip)]
    cast: Option<CastTarget>,
    type_name: Option<TypeName>,
    #[getter(skip)]
    domain_constraint: Option<DomainConstraintTarget>,
    #[getter(skip)]
    transform: Option<TransformTarget>,
}

#[derive(Clone, Debug)]
pub(crate) struct CastTarget {
    source: TypeName,
    target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct DomainConstraintTarget {
    domain: TypeName,
    constraint: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformTarget {
    type_name: TypeName,
    language: String,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct ObjectArgument {
    #[getter(skip)]
    mode: Option<String>,
    #[getter(skip)]
    name: Option<String>,
    type_name: Option<TypeName>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Role {
    #[getter(copy)]
    kind: u32,
    name: String,
}

impl AlterOwner {
    const fn new(
        object_kind: Option<ObjectKind>,
        relation: Option<Name>,
        object: Option<ObjectTarget>,
        owner: Role,
    ) -> Self {
        Self {
            object_kind,
            relation,
            object,
            owner,
        }
    }
}

impl Role {
    fn new(kind: u32, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Comment {
    #[getter(skip)]
    target: ObjectTarget,
    #[getter(copy)]
    object_kind: ObjectKind,
    text: String,
}

impl Comment {
    fn new(target: ObjectTarget, object_kind: ObjectKind, text: &str) -> Self {
        Self {
            target,
            object_kind,
            text: text.to_owned(),
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct AlterSequence {
    name: Name,
    options: Vec<SyntaxOption>,
    #[getter(skip)]
    for_identity: bool,
    #[getter(skip)]
    missing_ok: bool,
}

impl AlterSequence {
    const fn new(
        name: Name,
        options: Vec<SyntaxOption>,
        for_identity: bool,
        missing_ok: bool,
    ) -> Self {
        Self {
            name,
            options,
            for_identity,
            missing_ok,
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct DropTable {
    names: Vec<Name>,
    #[getter(copy)]
    if_exists: bool,
    #[getter(copy)]
    cascade: bool,
    #[getter(skip)]
    concurrent: bool,
}

impl DropTable {
    const fn new(names: Vec<Name>, if_exists: bool, cascade: bool, concurrent: bool) -> Self {
        Self {
            names,
            if_exists,
            cascade,
            concurrent,
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Sequence {
    #[getter(copy)]
    temporary: bool,
    name: Name,
    options: Vec<SyntaxOption>,
    #[getter(skip)]
    owner_id: u32,
    #[getter(skip)]
    for_identity: bool,
    #[getter(copy)]
    if_not_exists: bool,
}

impl Sequence {
    const fn with_temporary(mut self, value: bool) -> Self {
        self.temporary = value;
        self
    }
    const fn new(
        name: Name,
        options: Vec<SyntaxOption>,
        owner_id: u32,
        for_identity: bool,
        if_not_exists: bool,
    ) -> Self {
        Self {
            temporary: false,
            name,
            options,
            owner_id,
            for_identity,
            if_not_exists,
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct EnumType {
    name: Name,
    values: Vec<String>,
}

impl EnumType {
    const fn new(name: Name, values: Vec<String>) -> Self {
        Self { name, values }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct VariableSet {
    #[getter(skip)]
    kind: VariableSetKind,
    #[getter(skip)]
    is_local: bool,
    name: String,
    arguments: Vec<Expr>,
    #[getter(skip)]
    transaction_options: Option<Vec<SyntaxOption>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum VariableSetKind {
    Value,
    Default,
    Current,
    Multi,
    Reset,
    ResetAll,
}

impl VariableSet {
    fn new(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            kind: VariableSetKind::Value,
            is_local: false,
            name: name.to_owned(),
            arguments,
            transaction_options: None,
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Extension {
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(skip)]
    options: Vec<SyntaxOption>,
    name: String,
}

impl Extension {
    fn new(name: &str) -> Self {
        Self {
            if_not_exists: false,
            options: Vec::new(),
            name: name.to_owned(),
        }
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct View {
    name: Name,
    aliases: Vec<String>,
    query: Query,
    #[getter(copy)]
    replace: bool,
    options: Vec<SyntaxOption>,
    #[getter(copy)]
    check_option: ViewCheckOption,
    #[getter(copy)]
    temporary: bool,
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(copy)]
    materialized: bool,
}

impl View {
    #[expect(clippy::too_many_arguments)]
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
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ViewCheckOption {
    None,
    Local,
    Cascaded,
}

pub(crate) trait TryFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens(value: T, tokens: &IVec<ScanToken>) -> Result<Self, Self::Error>;
}

pub(crate) trait TryFromWithTokensOpt<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens_opt(
        value: T,
        tokens: &IVec<ScanToken>,
    ) -> Result<Option<Self>, Self::Error>;
}

pub(crate) trait TryFromOpt<T>: TryFrom<T> {
    fn try_from_opt(value: Option<T>) -> Result<Option<Self>, <Self as TryFrom<T>>::Error>
    where
        Self: Sized,
    {
        value.map(Self::try_from).transpose()
    }
}

impl<T, U> TryFromOpt<T> for U where U: TryFrom<T> {}

pub(crate) trait TryFromList<T>: Sized {
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

pub(crate) trait FromList<T>: Sized {
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

pub(crate) trait FromOpt<T>: Sized {
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

pub(crate) trait TryCollectFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_collect_from_with_tokens(
        nodes: T,
        tokens: &IVec<ScanToken>,
    ) -> Result<Vec<Self>, Self::Error>;
}

/// SQL dialect explicitly selected for parsing and AST editing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    /// PostgreSQL SQL syntax.
    Postgres,
    /// SQLite SQL syntax; parsing requires the `sqlite` feature.
    Sqlite,
}

impl Dialect {
    /// Select PostgreSQL.
    pub const fn postgres() -> Self {
        Self::Postgres
    }
    /// Select SQLite.
    pub const fn sqlite() -> Self {
        Self::Sqlite
    }
}

impl std::fmt::Display for Dialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Postgres => "PostgreSQL",
            Self::Sqlite => "SQLite",
        })
    }
}

impl From<&::sql_fun_core::SqlFunMetadata> for Dialect {
    fn from(metadata: &::sql_fun_core::SqlFunMetadata) -> Self {
        match metadata.sql_dialect() {
            ::sql_fun_core::SqlDialect::PostgreSQL => Self::postgres(),
            #[cfg(feature = "sqlite")]
            ::sql_fun_core::SqlDialect::SQLite => Self::sqlite(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Name(Vec<String>);

impl Name {
    pub(crate) fn parts(&self) -> &[String] {
        &self.0
    }

    pub(crate) fn from_parts(parts: &[String]) -> Self {
        Self(parts.to_vec())
    }

    pub(crate) fn local(value: &str) -> Self {
        Self(vec![value.to_owned()])
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TypeName {
    name: Name,
    modifiers: Vec<Expr>,
    array_bounds: Vec<Option<i32>>,
    #[getter(copy)]
    set_of: bool,
    #[getter(copy)]
    percent_type: bool,
    #[getter(skip)]
    sqlite_name_snapshot: Option<Name>,
    #[getter(skip)]
    sqlite_parts: Vec<SqliteTypeNamePart>,
    #[getter(skip)]
    sqlite_modifiers: Option<SqliteTypeModifiers>,
    #[getter(skip)]
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeNamePart {
    name: String,
    quote: Option<char>,
    leading_trivia: Vec<SqliteTypeNameTrivia>,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifiers {
    values: Vec<SqliteTypeModifier>,
    ast_spellings: Vec<String>,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifier {
    leading_trivia: Vec<SqliteTypeNameTrivia>,
    spelling: String,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) enum SqliteTypeNameTrivia {
    Whitespace(String),
    BlockComment(String),
    LineComment(String),
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

impl Literal {
    const fn null() -> Self {
        Self::Null
    }
}

impl Expr {
    pub(in crate::ast) const fn null() -> Self {
        Self::Literal(Literal::null())
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CurrentOf {
    name: String,
    #[getter(copy)]
    parameter: i32,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Parameter {
    location: Option<StringSpan>,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct OperatorExpr {
    name: Name,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct BooleanExpr {
    #[getter(copy)]
    operator: BooleanOperator,
    arguments: Vec<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BooleanOperator {
    And,
    Or,
    Not,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct NullTest {
    argument: Expr,
    #[getter(copy)]
    negated: bool,
    #[getter(skip)]
    row: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TruthTest {
    argument: Expr,
    #[getter(copy)]
    value: Option<bool>,
    #[getter(copy)]
    negated: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Cast {
    argument: Expr,
    target: TypeName,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Collate {
    argument: Expr,
    name: Name,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Call {
    #[getter(skip)]
    sql_syntax: bool,
    #[getter(copy)]
    kind: CallKind,
    name: Name,
    arguments: Vec<Expr>,
    argument_locations: Vec<Option<StringSpan>>,
    #[getter(copy)]
    star: bool,
    #[getter(copy)]
    distinct: bool,
    #[getter(copy)]
    variadic: bool,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Case {
    argument: Option<Expr>,
    arms: Vec<CaseArm>,
    otherwise: Option<Expr>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CaseArm {
    condition: Expr,
    result: Expr,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Subquery {
    query: Query,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Slice {
    lower: Option<Expr>,
    upper: Option<Expr>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct NamedArgument {
    name: String,
    argument: Expr,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct SqlValue {
    name: String,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Query {
    with: With,
    body: QueryBody,
    order: Vec<Order>,
    limit: Option<Expr>,
    offset: Option<Expr>,
    #[getter(copy)]
    with_ties: bool,
    locks: Vec<Lock>,
}

#[derive(Clone, Debug)]
pub(crate) enum QueryBody {
    Select(Box<Select>),
    Values(Vec<Vec<Expr>>),
    Set(Box<SetQuery>),
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct SetQuery {
    #[getter(copy)]
    operation: SetOperation,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Select {
    into: Option<Into>,
    distinct: Option<Vec<Expr>>,
    targets: Vec<Target>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    group: Vec<Grouping>,
    #[getter(copy)]
    group_distinct: bool,
    having: Option<Expr>,
    windows: Vec<Window>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Target {
    #[getter(copy)]
    implicit_alias: bool,
    #[getter(copy)]
    location: Option<usize>,
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Order {
    expression: Expr,
    #[getter(copy)]
    descending: Option<bool>,
    #[getter(copy)]
    nulls_first: Option<bool>,
    using: Option<Name>,
}

#[derive(Clone, Debug, Default, derive_getters::Getters)]
pub(crate) struct With {
    #[getter(copy)]
    recursive: bool,
    entries: Vec<Cte>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Cte {
    name: String,
    #[getter(skip)]
    columns: Vec<CteColumn>,
    #[getter(copy)]
    materialized: Option<bool>,
    body: CteBody,
    #[getter(skip)]
    search: Option<Search>,
    #[getter(skip)]
    cycle: Option<Cycle>,
}

#[derive(Clone, Debug)]
pub(crate) struct CteColumn {
    name: String,
    collation: Option<String>,
    descending: Option<bool>,
}

impl Cte {
    pub(crate) fn column_names(&self) -> Vec<String> {
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

#[derive(Clone, Debug, derive_getters::Getters)]
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
    pub(crate) const fn relation(relation: Relation) -> Self {
        Self::Relation(relation)
    }

    pub(crate) fn query(query: DerivedTable) -> Self {
        Self::Query(Box::new(query))
    }

    pub(crate) fn join(join: Join) -> Self {
        Self::Join(Box::new(join))
    }

    pub(crate) const fn functions(functions: TableFunctions) -> Self {
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Relation {
    name: Name,
    alias: Option<Alias>,

    #[getter(copy)]
    inherit: bool,

    persistence: String,
    indexed: Option<String>,
    #[getter(copy)]
    not_indexed: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct DerivedTable {
    query: Query,
    alias: Option<Alias>,
    #[getter(copy)]
    lateral: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct FromGroup {
    items: Vec<FromItem>,
    alias: Option<Alias>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Join {
    left: FromItem,
    right: FromItem,
    #[getter(copy)]
    kind: JoinKind,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TableFunctions {
    calls: Vec<TableFunction>,
    alias: Option<Alias>,
    #[getter(skip)]
    lateral: bool,
    #[getter(copy)]
    ordinality: bool,
    #[getter(copy)]
    rows_from: bool,
    columns: Vec<TypedColumn>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TypedColumn {
    name: String,
    type_name: TypeName,
    collation: Option<Name>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TableSample {
    relation: FromItem,
    method: Name,
    arguments: Vec<Expr>,
    repeatable: Option<Expr>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Window {
    #[getter(copy)]
    reference_only: bool,
    name: Option<String>,
    reference: Option<String>,
    partition: Vec<Expr>,
    order: Vec<Order>,
    frame: Option<Frame>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Frame {
    #[getter(copy)]
    mode: FrameMode,
    start: FrameBound,
    end: Option<FrameBound>,
    #[getter(copy)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Lock {
    relations: Vec<Name>,
    #[getter(copy)]
    strength: Option<LockStrength>,
    #[getter(copy)]
    wait: Option<LockWait>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Assignment {
    name: String,
    indirection: Vec<Access>,
    value: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReturningOption {
    old: bool,
    alias: String,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Insert {
    with: With,
    relation: Relation,
    columns: Vec<Assignment>,
    source: Option<Query>,
    #[getter(copy)]
    overriding: Option<OverridingKind>,
    conflict: Option<Conflict>,
    or_conflict: Option<ConflictClause>,
    returning: Vec<Target>,
    returning_options: Vec<ReturningOption>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Conflict {
    #[getter(copy)]
    action: Option<ConflictAction>,
    inference: Vec<IndexExpression>,
    inference_predicate: Option<Expr>,
    constraint: Option<String>,
    #[getter(skip)]
    assignments: Vec<Assignment>,
    #[getter(skip)]
    predicate: Option<Expr>,
    next: Option<Box<Self>>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct IndexExpression {
    name: Option<String>,
    alias: Option<String>,
    expression: Option<Expr>,
    collation: Name,
    operator_class: Name,
    #[getter(skip)]
    options: Vec<SyntaxOption>,
    #[getter(copy)]
    descending: Option<bool>,
    #[getter(copy)]
    using_order: bool,
    #[getter(skip)]
    nulls_first: Option<bool>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Into {
    relation: Relation,

    columns: Vec<String>,
    #[getter(skip)]
    access_method: Option<String>,
    #[getter(skip)]
    options: Vec<SyntaxOption>,
    #[getter(skip)]
    on_commit: Option<CommitAction>,
    #[getter(skip)]
    tablespace: Option<String>,
    #[getter(skip)]
    skip_data: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
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

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Update {
    with: With,
    relation: Relation,
    assignments: Vec<Assignment>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
    returning_options: Vec<ReturningOption>,
    or_conflict: Option<ConflictClause>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Delete {
    with: With,
    relation: Relation,
    using: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
    returning_options: Vec<ReturningOption>,
}

#[cfg(test)]
impl Delete {
    pub(crate) fn set_current_of_parameter(&mut self, parameter: i32) {
        let Some(Expr::CurrentOf(cursor)) = &mut self.predicate else {
            panic!("CURRENT OF predicate required");
        };
        cursor.parameter = parameter;
    }
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

// Unresolved PostgreSQL rule syntax; no catalog or writable-view assumptions.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CreateRule {
    name: Name,
    relation: Relation,
    #[getter(copy)]
    replace: bool,
    #[getter(copy)]
    event: RuleEvent,
    predicate: Option<Expr>,
    #[getter(copy)]
    instead: bool,
    actions: Vec<StatementNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleEvent {
    Select,
    Insert,
    Update,
    Delete,
}

// Unresolved PostgreSQL DROP RULE syntax; the ON relation is always explicit.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct DropRule {
    name: Name,
    relation: Name,
    #[getter(copy)]
    if_exists: bool,
    #[getter(copy)]
    behavior: DropBehavior,
}

// DROP TRIGGER retains only syntax; SQLite has no ON relation or drop behavior.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct DropTrigger {
    name: Name,
    relation: Option<Name>,
    #[getter(copy)]
    if_exists: bool,
    #[getter(copy)]
    behavior: Option<DropBehavior>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropBehavior {
    Cascade,
    Restrict,
}

// Unresolved trigger syntax; interpretation belongs to statement-specific analyzers.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CreateTrigger {
    name: Name,
    relation: Relation,
    #[getter(copy)]
    replace: bool,
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(copy)]
    temporary: bool,
    #[getter(copy)]
    timing: Option<TriggerTiming>,
    events: Vec<TriggerEvent>,
    #[getter(copy)]
    granularity: Option<TriggerGranularity>,
    predicate: Option<Expr>,
    constraint: Option<TriggerConstraint>,
    transitions: Vec<TriggerTransition>,
    body: TriggerBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TriggerGranularity {
    Row,
    Statement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TriggerEvent {
    Insert,
    Update(Vec<String>),
    Delete,
    Truncate,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TriggerConstraint {
    relation: Option<Relation>,
    #[getter(copy)]
    deferrable: bool,
    #[getter(copy)]
    initially_deferred: bool,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TriggerTransition {
    name: String,
    #[getter(skip)]
    new: bool,
    #[getter(copy)]
    table: bool,
}

impl TriggerTransition {
    pub(crate) const fn is_new(&self) -> bool {
        self.new
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TriggerBody {
    Function(TriggerFunction),
    Commands(Vec<StatementNode>),
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TriggerFunction {
    name: Name,
    arguments: Vec<String>,
}

#[cfg(test)]
impl Query {
    pub(crate) fn test_rows_functions(self) -> TableFunctions {
        let QueryBody::Select(mut select) = self.body else {
            panic!("select");
        };
        let FromItem::Functions(source) = select.from.pop().unwrap() else {
            panic!("function");
        };
        source
    }

    pub(crate) fn test_projection_statement(source: FromItem) -> Statement {
        Statement::from_node(
            StatementNode::query(Self::projection(&[], &[source])),
            Dialect::postgres(),
            StringSpan::default(),
        )
    }
}

#[cfg(test)]
impl StatementNode {
    pub(crate) fn into_test_drop_rule(self) -> DropRule {
        match self {
            Self::DropRule(rule) => rule,
            _ => panic!("DROP RULE node"),
        }
    }
}

#[cfg(test)]
impl StatementNode {
    pub(crate) fn into_test_drop_trigger(self) -> DropTrigger {
        match self {
            Self::DropTrigger(trigger) => trigger,
            _ => panic!("DROP TRIGGER node"),
        }
    }
}

#[cfg(test)]
impl TableConstraint {
    pub(crate) fn test_new(
        name: Option<&str>,
        keys: &[&str],
        constraint: ColumnConstraint,
    ) -> Self {
        Self {
            extras: ConstraintExtras::default(),
            name: name.map(str::to_owned),
            keys: keys.iter().map(|key| (*key).to_owned()).collect(),
            constraint,
            order: Vec::new(),
            auto_increment: false,
        }
    }
}

impl ConstraintExtras {
    pub(crate) fn explicit_enforcement(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Option<bool> {
        if !matches!(
            value.get_contype().as_inner(),
            Some(syn::ConstrType::ConstrCheck | syn::ConstrType::ConstrForeign)
        ) {
            return None;
        }
        let mut depth = 0;
        for token in tokens
            .iter()
            .filter(|token| token.get_start() >= value.get_location())
        {
            match token.get_token().as_inner() {
                Some(syn::Token::Ascii40) => depth += 1,
                Some(syn::Token::Ascii41) if depth == 0 => break,
                Some(syn::Token::Ascii41) => depth -= 1,
                Some(syn::Token::Ascii44) if depth == 0 => break,
                Some(syn::Token::Enforced) if depth == 0 => return Some(value.get_is_enforced()),
                _ => {}
            }
        }
        None
    }
}

impl ObjectArgument {
    pub(crate) fn is_output_only(&self) -> bool {
        self.mode.as_deref() == Some("OUT") || self.mode.as_deref() == Some("TABLE")
    }
}

// Location synchronization for syntax-only projection edits.

/// A rejected synchronization of syntax projection locations.
/// The destination statement remains unchanged on failure.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TargetLocationError(TargetLocationProblem);

#[derive(Debug, thiserror::Error)]
enum TargetLocationProblem {
    #[error("wildcard location was not found")]
    NotFound,
    #[error("wildcard location is ambiguous")]
    Ambiguous,
    #[error("{0}")]
    InvalidExpression(String),
}

impl TargetLocationError {
    pub(crate) fn invalid(message: &str) -> Self {
        Self(TargetLocationProblem::invalid(message))
    }
    /// Whether the parser output has a target without a lexical location.
    pub const fn is_not_found(&self) -> bool {
        self.0.is_not_found()
    }
    /// Whether multiple parser targets share a lexical location.
    pub const fn is_ambiguous(&self) -> bool {
        self.0.is_ambiguous()
    }
    /// Whether the statements cannot be rendered or matched for synchronization.
    pub const fn is_invalid_expression(&self) -> bool {
        self.0.is_invalid_expression()
    }
}

impl TargetLocationProblem {
    const fn not_found() -> Self {
        Self::NotFound
    }
    const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    fn invalid(message: &str) -> Self {
        Self::InvalidExpression(message.to_owned())
    }
    const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    const fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }
    const fn is_invalid_expression(&self) -> bool {
        matches!(self, Self::InvalidExpression(_))
    }
}

impl From<ProjectionError> for TargetLocationError {
    fn from(error: ProjectionError) -> Self {
        if error.is_missing() {
            Self(TargetLocationProblem::not_found())
        } else if error.is_ambiguous() {
            Self(TargetLocationProblem::ambiguous())
        } else {
            Self::invalid("target collections differ after the parser round trip")
        }
    }
}

impl FromItem {
    pub(crate) fn clear_projection_alias(&mut self) {
        match self {
            Self::Join(node) => node.clear_projection_alias(),
            Self::Group(node) => node.clear_projection_alias(),
            _ => {}
        }
    }
}
impl Join {
    fn clear_projection_alias(&mut self) {
        self.alias = None;
    }
}
impl FromGroup {
    fn clear_projection_alias(&mut self) {
        self.alias = None;
    }
}
