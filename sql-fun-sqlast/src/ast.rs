//! Context-free SQL syntax shared by the parser frontends.
//!
//! Names in this module are spelling, not catalog bindings. In particular, type
//! names and function names do not imply that an object exists. Recursive edges
//! always stay in this representation, including expressions inside query clauses.
//! Converters consume newly built child nodes to avoid cloning recursive trees;
//! ordinary semantic factories borrow their inputs.

#![allow(dead_code)]

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
mod utility;
pub(crate) mod validation;

pub(crate) use self::{
    ddl::{CompositeType, Domain, Function, FunctionParameter, Index, Schema, TableAs},
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
    pub(crate) unlogged: bool,
    pub(crate) inherits: Vec<Name>,
    pub(crate) of_type: Option<TypeName>,
    pub(crate) on_commit: Option<CommitAction>,
    pub(crate) tablespace: Option<String>,
    pub(crate) access_method: Option<String>,
    pub(crate) partition_spec: Option<PartitionSpec>,
    pub(crate) partition_bound: Option<PartitionBound>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionSpec {
    pub(crate) strategy: String,
    pub(crate) keys: Vec<IndexExpression>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionBound {
    pub(crate) strategy: String,
    pub(crate) is_default: bool,
    pub(crate) modulus: i32,
    pub(crate) remainder: i32,
    pub(crate) list: Vec<Expr>,
    pub(crate) lower: Vec<PartitionDatum>,
    pub(crate) upper: Vec<PartitionDatum>,
}
#[derive(Clone, Debug)]
pub(crate) enum PartitionDatum {
    Min,
    Max,
    Value(Expr),
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ConstraintExtras {
    pub(crate) enforced: Option<bool>,
    pub(crate) without_overlaps: bool,
    pub(crate) fk_with_period: bool,
    pub(crate) pk_with_period: bool,
    pub(crate) deferrable: bool,
    pub(crate) initially_deferred: bool,
    pub(crate) skip_validation: bool,
    pub(crate) no_inherit: bool,
    pub(crate) nulls_not_distinct: bool,
    pub(crate) include: Vec<String>,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) index_name: Option<String>,
    pub(crate) tablespace: Option<String>,
    pub(crate) access_method: Option<String>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) exclusions: Vec<ExclusionElement>,
    pub(crate) delete_columns: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct ExclusionElement {
    pub(crate) expression: IndexExpression,
    pub(crate) operator: Name,
}

#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub(crate) extras: TableExtras,
    pub(crate) name: Name,
    pub(crate) columns: Vec<Column>,
    pub(crate) constraints: Vec<TableConstraint>,
    pub(crate) temporary: bool,
    pub(crate) mode: CreateTableMode,
    pub(crate) query: Option<Query>,
    pub(crate) options: Vec<SyntaxOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct Column {
    pub(crate) collation: Name,
    pub(crate) compression: Option<String>,
    pub(crate) storage: Option<String>,
    pub(crate) name: String,
    pub(crate) type_name: Option<TypeName>,
    pub(crate) type_mod: Option<i32>,
    pub(crate) constraints: Vec<TableConstraint>,
    pub(crate) not_null: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableConstraint {
    pub(crate) extras: ConstraintExtras,
    pub(crate) name: Option<String>,
    pub(crate) keys: Vec<String>,
    pub(crate) constraint: ColumnConstraint,
    pub(crate) order: Vec<Order>,
    pub(crate) auto_increment: bool,
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

#[derive(Clone, Debug)]
pub(crate) struct ForeignReference {
    pub(crate) table: Name,
    pub(crate) columns: Vec<String>,
    pub(crate) match_type: Option<ForeignKeyMatchType>,
    pub(crate) delete_action: Option<ForeignKeyAction>,
    pub(crate) update_action: Option<ForeignKeyAction>,
    pub(crate) deferrability: Option<Deferrability>,
    pub(crate) validity: Option<Validity>,
    pub(crate) ignored_matches: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct AlterRelation {
    pub(crate) name: Name,
    pub(crate) kind: AlterRelationKind,
    pub(crate) commands: Vec<AlterCommand>,
    pub(crate) if_exists: bool,
    pub(crate) only: bool,
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

#[derive(Clone, Debug)]
pub(crate) struct AlterOwner {
    pub(crate) object_kind: Option<ObjectKind>,
    pub(crate) relation: Option<Name>,
    pub(crate) object: Option<ObjectTarget>,
    pub(crate) owner: Role,
}

/// A named database object, optionally identified by its argument type list.
/// `None` distinguishes an unspecified signature from an explicitly empty `()`.
#[derive(Clone, Debug)]
pub(crate) struct ObjectTarget {
    pub(crate) name: Vec<String>,
    pub(crate) argument_types: Option<Vec<ObjectArgument>>,
    pub(crate) cast: Option<CastTarget>,
    pub(crate) type_name: Option<TypeName>,
    pub(crate) domain_constraint: Option<DomainConstraintTarget>,
    pub(crate) transform: Option<TransformTarget>,
}

#[derive(Clone, Debug)]
pub(crate) struct CastTarget {
    pub(crate) source: TypeName,
    pub(crate) target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct DomainConstraintTarget {
    pub(crate) domain: TypeName,
    pub(crate) constraint: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformTarget {
    pub(crate) type_name: TypeName,
    pub(crate) language: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ObjectArgument {
    pub(crate) mode: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) type_name: Option<TypeName>,
}

#[derive(Clone, Debug)]
pub(crate) struct Role {
    pub(crate) kind: u32,
    pub(crate) name: String,
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

#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub(crate) target: ObjectTarget,
    pub(crate) object_kind: ObjectKind,
    pub(crate) text: String,
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

#[derive(Clone, Debug)]
pub(crate) struct AlterSequence {
    pub(crate) name: Name,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) for_identity: bool,
    pub(crate) missing_ok: bool,
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

#[derive(Clone, Debug)]
pub(crate) struct DropTable {
    pub(crate) names: Vec<Name>,
    pub(crate) if_exists: bool,
    pub(crate) cascade: bool,
    pub(crate) concurrent: bool,
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

#[derive(Clone, Debug)]
pub(crate) struct Sequence {
    pub(crate) temporary: bool,
    pub(crate) name: Name,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) owner_id: u32,
    pub(crate) for_identity: bool,
    pub(crate) if_not_exists: bool,
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

#[derive(Clone, Debug)]
pub(crate) struct EnumType {
    pub(crate) name: Name,
    pub(crate) values: Vec<String>,
}

impl EnumType {
    const fn new(name: Name, values: Vec<String>) -> Self {
        Self { name, values }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VariableSet {
    pub(crate) kind: VariableSetKind,
    pub(crate) is_local: bool,
    pub(crate) name: String,
    pub(crate) arguments: Vec<Expr>,
    pub(crate) transaction_options: Option<Vec<SyntaxOption>>,
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

#[derive(Clone, Debug)]
pub(crate) struct Extension {
    pub(crate) if_not_exists: bool,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) name: String,
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

#[derive(Clone, Debug)]
pub(crate) struct View {
    pub(crate) name: Name,
    pub(crate) aliases: Vec<String>,
    pub(crate) query: Query,
    pub(crate) replace: bool,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) check_option: ViewCheckOption,
    pub(crate) temporary: bool,
    pub(crate) if_not_exists: bool,
    pub(crate) materialized: bool,
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
pub(crate) struct Name(pub(crate) Vec<String>);

impl Name {
    pub(crate) fn from_parts(parts: &[String]) -> Self {
        Self(parts.to_vec())
    }

    pub(crate) fn local(value: &str) -> Self {
        Self(vec![value.to_owned()])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TypeName {
    pub(crate) name: Name,
    pub(crate) modifiers: Vec<Expr>,
    pub(crate) array_bounds: Vec<Option<i32>>,
    pub(crate) set_of: bool,
    pub(crate) percent_type: bool,
    pub(crate) sqlite_name_snapshot: Option<Name>,
    pub(crate) sqlite_parts: Vec<SqliteTypeNamePart>,
    pub(crate) sqlite_modifiers: Option<SqliteTypeModifiers>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeNamePart {
    pub(crate) name: String,
    pub(crate) quote: Option<char>,
    pub(crate) leading_trivia: Vec<SqliteTypeNameTrivia>,
    pub(crate) trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifiers {
    pub(crate) values: Vec<SqliteTypeModifier>,
    pub(crate) ast_spellings: Vec<String>,
    pub(crate) trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifier {
    pub(crate) leading_trivia: Vec<SqliteTypeNameTrivia>,
    pub(crate) spelling: String,
    pub(crate) trailing_trivia: Vec<SqliteTypeNameTrivia>,
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

#[derive(Clone, Debug)]
pub(crate) struct CurrentOf {
    pub(crate) name: String,
    pub(crate) parameter: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    pub(crate) position: u32,
    pub(crate) name: Option<String>,
    pub(crate) type_name: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct InTable {
    pub(crate) argument: Expr,
    pub(crate) negated: bool,
    pub(crate) table: Name,
    pub(crate) arguments: Vec<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorExpr {
    pub(crate) name: Name,
    pub(crate) kind: OperatorKind,
    pub(crate) left: Option<Expr>,
    pub(crate) right: Option<Expr>,
    pub(crate) escape: Option<Expr>,
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
    pub(crate) operator: BooleanOperator,
    pub(crate) arguments: Vec<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BooleanOperator {
    And,
    Or,
    Not,
}

#[derive(Clone, Debug)]
pub(crate) struct NullTest {
    pub(crate) argument: Expr,
    pub(crate) negated: bool,
    pub(crate) row: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TruthTest {
    pub(crate) argument: Expr,
    pub(crate) value: Option<bool>,
    pub(crate) negated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Cast {
    pub(crate) argument: Expr,
    pub(crate) target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct Collate {
    pub(crate) argument: Expr,
    pub(crate) name: Name,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct Call {
    pub(crate) sql_syntax: bool,
    pub(crate) kind: CallKind,
    pub(crate) name: Name,
    pub(crate) arguments: Vec<Expr>,
    pub(crate) argument_locations: Vec<Option<StringSpan>>,
    pub(crate) star: bool,
    pub(crate) distinct: bool,
    pub(crate) variadic: bool,
    pub(crate) within_group: bool,
    pub(crate) within_order: Vec<Order>,
    pub(crate) order: Vec<Order>,
    pub(crate) filter: Option<Expr>,
    pub(crate) over: Option<Window>,
    pub(crate) location: Option<StringSpan>,
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
    pub(crate) argument: Option<Expr>,
    pub(crate) arms: Vec<CaseArm>,
    pub(crate) otherwise: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct CaseArm {
    pub(crate) condition: Expr,
    pub(crate) result: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct Subquery {
    pub(crate) query: Query,
    pub(crate) kind: SubqueryKind,
    pub(crate) test: Option<Expr>,
    pub(crate) operator: Name,
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
    pub(crate) argument: Expr,
    pub(crate) items: Vec<Access>,
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
    pub(crate) lower: Option<Expr>,
    pub(crate) upper: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct NamedArgument {
    pub(crate) name: String,
    pub(crate) argument: Expr,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqlValue {
    pub(crate) name: String,
    pub(crate) precision: Option<i32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Raise {
    pub(crate) action: RaiseAction,
    pub(crate) message: Option<Expr>,
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
    pub(crate) with: With,
    pub(crate) body: QueryBody,
    pub(crate) order: Vec<Order>,
    pub(crate) limit: Option<Expr>,
    pub(crate) offset: Option<Expr>,
    pub(crate) with_ties: bool,
    pub(crate) locks: Vec<Lock>,
}

#[derive(Clone, Debug)]
pub(crate) enum QueryBody {
    Select(Box<Select>),
    Values(Vec<Vec<Expr>>),
    Set(Box<SetQuery>),
}

#[derive(Clone, Debug)]
pub(crate) struct SetQuery {
    pub(crate) operation: SetOperation,
    pub(crate) all: bool,
    pub(crate) left: Query,
    pub(crate) right: Query,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SetOperation {
    Union,
    Intersect,
    Except,
}

#[derive(Clone, Debug)]
pub(crate) struct Select {
    pub(crate) into: Option<Into>,
    pub(crate) distinct: Option<Vec<Expr>>,
    pub(crate) targets: Vec<Target>,
    pub(crate) from: Vec<FromItem>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) group: Vec<Grouping>,
    pub(crate) group_distinct: bool,
    pub(crate) having: Option<Expr>,
    pub(crate) windows: Vec<Window>,
}

#[derive(Clone, Debug)]
pub(crate) struct Target {
    pub(crate) implicit_alias: bool,
    pub(crate) location: Option<usize>,
    pub(crate) expression: Expr,
    pub(crate) alias: Option<String>,
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
    pub(crate) expression: Expr,
    pub(crate) descending: Option<bool>,
    pub(crate) nulls_first: Option<bool>,
    pub(crate) using: Option<Name>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct With {
    pub(crate) recursive: bool,
    pub(crate) entries: Vec<Cte>,
}

#[derive(Clone, Debug)]
pub(crate) struct Cte {
    pub(crate) name: String,
    pub(crate) columns: Vec<CteColumn>,
    pub(crate) materialized: Option<bool>,
    pub(crate) body: CteBody,
    pub(crate) search: Option<Search>,
    pub(crate) cycle: Option<Cycle>,
}

#[derive(Clone, Debug)]
pub(crate) struct CteColumn {
    pub(crate) name: String,
    pub(crate) collation: Option<String>,
    pub(crate) descending: Option<bool>,
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
    pub(crate) columns: Vec<String>,
    pub(crate) breadth_first: bool,
    pub(crate) sequence: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Cycle {
    pub(crate) columns: Vec<String>,
    pub(crate) mark: String,
    pub(crate) value: Option<Expr>,
    pub(crate) default: Option<Expr>,
    pub(crate) path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Alias {
    pub(crate) name: String,
    pub(crate) columns: Vec<String>,
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

#[derive(Clone, Debug)]
pub(crate) struct Relation {
    pub(crate) name: Name,
    pub(crate) alias: Option<Alias>,
    pub(crate) inherit: bool,
    pub(crate) persistence: String,
    pub(crate) indexed: Option<String>,
    pub(crate) not_indexed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DerivedTable {
    pub(crate) query: Query,
    pub(crate) alias: Option<Alias>,
    pub(crate) lateral: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FromGroup {
    pub(crate) items: Vec<FromItem>,
    pub(crate) alias: Option<Alias>,
}

#[derive(Clone, Debug)]
pub(crate) struct Join {
    pub(crate) left: FromItem,
    pub(crate) right: FromItem,
    pub(crate) kind: JoinKind,
    pub(crate) natural: bool,
    pub(crate) condition: Option<Expr>,
    pub(crate) using: Vec<String>,
    pub(crate) using_alias: Option<Alias>,
    pub(crate) alias: Option<Alias>,
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
    pub(crate) calls: Vec<TableFunction>,
    pub(crate) alias: Option<Alias>,
    pub(crate) lateral: bool,
    pub(crate) ordinality: bool,
    pub(crate) rows_from: bool,
    pub(crate) columns: Vec<TypedColumn>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableFunction {
    pub(crate) expression: Expr,
    pub(crate) columns: Vec<TypedColumn>,
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
    pub(crate) name: String,
    pub(crate) type_name: TypeName,
    pub(crate) collation: Option<Name>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableSample {
    pub(crate) relation: FromItem,
    pub(crate) method: Name,
    pub(crate) arguments: Vec<Expr>,
    pub(crate) repeatable: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Window {
    pub(crate) reference_only: bool,
    pub(crate) name: Option<String>,
    pub(crate) reference: Option<String>,
    pub(crate) partition: Vec<Expr>,
    pub(crate) order: Vec<Order>,
    pub(crate) frame: Option<Frame>,
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) mode: FrameMode,
    pub(crate) start: FrameBound,
    pub(crate) end: Option<FrameBound>,
    pub(crate) exclusion: Option<FrameExclusion>,
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
    pub(crate) relations: Vec<Name>,
    pub(crate) strength: Option<LockStrength>,
    pub(crate) wait: Option<LockWait>,
}

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    pub(crate) name: String,
    pub(crate) indirection: Vec<Access>,
    pub(crate) value: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReturningOption {
    pub(crate) old: bool,
    pub(crate) alias: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Insert {
    pub(crate) with: With,
    pub(crate) relation: Relation,
    pub(crate) columns: Vec<Assignment>,
    pub(crate) source: Option<Query>,
    pub(crate) overriding: Option<OverridingKind>,
    pub(crate) conflict: Option<Conflict>,
    pub(crate) or_conflict: Option<ConflictClause>,
    pub(crate) returning: Vec<Target>,
    pub(crate) returning_options: Vec<ReturningOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    pub(crate) action: Option<ConflictAction>,
    pub(crate) inference: Vec<IndexExpression>,
    pub(crate) inference_predicate: Option<Expr>,
    pub(crate) constraint: Option<String>,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) next: Option<Box<Self>>,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexExpression {
    pub(crate) name: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) expression: Option<Expr>,
    pub(crate) collation: Name,
    pub(crate) operator_class: Name,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) descending: Option<bool>,
    pub(crate) using_order: bool,
    pub(crate) nulls_first: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct Into {
    pub(crate) relation: Relation,
    pub(crate) columns: Vec<String>,
    pub(crate) access_method: Option<String>,
    pub(crate) options: Vec<SyntaxOption>,
    pub(crate) on_commit: Option<CommitAction>,
    pub(crate) tablespace: Option<String>,
    pub(crate) skip_data: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SyntaxOption {
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
    pub(crate) value: Option<SyntaxValue>,
}

#[derive(Clone, Debug)]
pub(crate) enum SyntaxValue {
    Expression(Expr),
    Type(TypeName),
}

#[derive(Clone, Debug)]
pub(crate) struct Update {
    pub(crate) with: With,
    pub(crate) relation: Relation,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) from: Vec<FromItem>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) returning: Vec<Target>,
    pub(crate) returning_options: Vec<ReturningOption>,
    pub(crate) or_conflict: Option<ConflictClause>,
}

#[derive(Clone, Debug)]
pub(crate) struct Delete {
    pub(crate) with: With,
    pub(crate) relation: Relation,
    pub(crate) using: Vec<FromItem>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) returning: Vec<Target>,
    pub(crate) returning_options: Vec<ReturningOption>,
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
#[derive(Clone, Debug)]
pub(crate) struct CreateRule {
    pub(crate) name: Name,
    pub(crate) relation: Relation,
    pub(crate) replace: bool,
    pub(crate) event: RuleEvent,
    pub(crate) predicate: Option<Expr>,
    pub(crate) instead: bool,
    pub(crate) actions: Vec<StatementNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleEvent {
    Select,
    Insert,
    Update,
    Delete,
}

// Unresolved PostgreSQL DROP RULE syntax; the ON relation is always explicit.
#[derive(Clone, Debug)]
pub(crate) struct DropRule {
    pub(crate) name: Name,
    pub(crate) relation: Name,
    pub(crate) if_exists: bool,
    pub(crate) behavior: DropBehavior,
}

// DROP TRIGGER retains only syntax; SQLite has no ON relation or drop behavior.
#[derive(Clone, Debug)]
pub(crate) struct DropTrigger {
    pub(crate) name: Name,
    pub(crate) relation: Option<Name>,
    pub(crate) if_exists: bool,
    pub(crate) behavior: Option<DropBehavior>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropBehavior {
    Cascade,
    Restrict,
}

// Unresolved trigger syntax; interpretation belongs to statement-specific analyzers.
#[derive(Clone, Debug)]
pub(crate) struct CreateTrigger {
    pub(crate) name: Name,
    pub(crate) relation: Relation,
    pub(crate) replace: bool,
    pub(crate) if_not_exists: bool,
    pub(crate) temporary: bool,
    pub(crate) timing: Option<TriggerTiming>,
    pub(crate) events: Vec<TriggerEvent>,
    pub(crate) granularity: Option<TriggerGranularity>,
    pub(crate) predicate: Option<Expr>,
    pub(crate) constraint: Option<TriggerConstraint>,
    pub(crate) transitions: Vec<TriggerTransition>,
    pub(crate) body: TriggerBody,
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

#[derive(Clone, Debug)]
pub(crate) struct TriggerConstraint {
    pub(crate) relation: Option<Relation>,
    pub(crate) deferrable: bool,
    pub(crate) initially_deferred: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TriggerTransition {
    pub(crate) name: String,
    pub(crate) new: bool,
    pub(crate) table: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum TriggerBody {
    Function(TriggerFunction),
    Commands(Vec<StatementNode>),
}

#[derive(Clone, Debug)]
pub(crate) struct TriggerFunction {
    pub(crate) name: Name,
    pub(crate) arguments: Vec<String>,
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
