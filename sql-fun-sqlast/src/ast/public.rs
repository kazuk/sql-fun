//! Supported inspection of the owned common AST.

use crate::ast::{Cte, CteBody, Dialect, StatementNode, With};

/// An owned, unresolved statement in an explicitly selected SQL dialect.
///
/// Parsing preserves syntax without resolving names against a catalog. Private
/// nodes retain additional syntax even where no inspection method is exposed yet.
#[derive(Clone, Debug)]
pub struct Statement {
    pub(super) node: StatementNode,
    pub(super) dialect: Dialect,
}

impl Statement {
    pub(crate) const fn from_node(node: StatementNode, dialect: Dialect) -> Self {
        Self { node, dialect }
    }

    /// Dialect explicitly chosen when this statement was parsed.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Kind of SQL statement, independent of the parser representation.
    pub const fn kind(&self) -> StatementKind {
        self.node.kind()
    }

    /// Whether this is a query (including VALUES and set operations).
    pub const fn is_query(&self) -> bool {
        self.node.is_query()
    }

    /// Whether this is an INSERT statement.
    pub const fn is_insert(&self) -> bool {
        self.node.is_insert()
    }

    /// Whether this is an UPDATE statement.
    pub const fn is_update(&self) -> bool {
        self.node.is_update()
    }

    /// Whether this is a DELETE statement.
    pub const fn is_delete(&self) -> bool {
        self.node.is_delete()
    }

    /// Inspect an existing WITH clause without exposing mutable node storage.
    pub fn with_clause(&self) -> Option<WithClause<'_>> {
        self.node
            .with_node()
            .and_then(|node| WithClause::existing(node, self.dialect))
    }
}

/// Parser-independent statement category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementKind {
    /// SELECT, VALUES, or a set operation.
    Query,
    /// INSERT.
    Insert,
    /// UPDATE.
    Update,
    /// DELETE.
    Delete,
    /// MERGE as a CTE body.
    Merge,
    /// A view definition.
    View,
    /// An extension definition.
    Extension,
    /// A variable assignment.
    VariableSet,
    /// An enum type definition.
    EnumType,
    /// A sequence definition.
    Sequence,
    /// DROP TABLE.
    DropTable,
    /// ALTER SEQUENCE.
    AlterSequence,
    /// COMMENT.
    Comment,
    /// An ownership change.
    AlterOwner,
    /// A composite type definition.
    CompositeType,
    /// A domain definition.
    Domain,
    /// A function definition.
    Function,
    /// A schema definition.
    Schema,
    /// An index definition.
    Index,
    /// CREATE TABLE AS.
    TableAs,
    /// A table definition.
    Table,
    /// ALTER TABLE or a related relation change.
    AlterRelation,
}

impl StatementKind {
    /// The query category.
    pub const fn query() -> Self {
        Self::Query
    }

    /// The insert category.
    pub const fn insert() -> Self {
        Self::Insert
    }

    /// The update category.
    pub const fn update() -> Self {
        Self::Update
    }

    /// The delete category.
    pub const fn delete() -> Self {
        Self::Delete
    }

    /// The merge category.
    pub const fn merge() -> Self {
        Self::Merge
    }

    /// The view category.
    pub const fn view() -> Self {
        Self::View
    }

    /// The extension category.
    pub const fn extension() -> Self {
        Self::Extension
    }

    /// The variable set category.
    pub const fn variable_set() -> Self {
        Self::VariableSet
    }

    /// The enum type category.
    pub const fn enum_type() -> Self {
        Self::EnumType
    }

    /// The sequence category.
    pub const fn sequence() -> Self {
        Self::Sequence
    }

    /// The drop table category.
    pub const fn drop_table() -> Self {
        Self::DropTable
    }

    /// The alter sequence category.
    pub const fn alter_sequence() -> Self {
        Self::AlterSequence
    }

    /// The comment category.
    pub const fn comment() -> Self {
        Self::Comment
    }

    /// The alter owner category.
    pub const fn alter_owner() -> Self {
        Self::AlterOwner
    }

    /// The composite type category.
    pub const fn composite_type() -> Self {
        Self::CompositeType
    }

    /// The domain category.
    pub const fn domain() -> Self {
        Self::Domain
    }

    /// The function category.
    pub const fn function() -> Self {
        Self::Function
    }

    /// The schema category.
    pub const fn schema() -> Self {
        Self::Schema
    }

    /// The index category.
    pub const fn index() -> Self {
        Self::Index
    }

    /// The table as category.
    pub const fn table_as() -> Self {
        Self::TableAs
    }

    /// The table category.
    pub const fn table() -> Self {
        Self::Table
    }

    /// The alter relation category.
    pub const fn alter_relation() -> Self {
        Self::AlterRelation
    }
}

impl StatementNode {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::View(_) => StatementKind::view(),
            Self::Extension(_) => StatementKind::extension(),
            Self::VariableSet(_) => StatementKind::variable_set(),
            Self::EnumType(_) => StatementKind::enum_type(),
            Self::Sequence(_) => StatementKind::sequence(),
            Self::DropTable(_) => StatementKind::drop_table(),
            Self::AlterSequence(_) => StatementKind::alter_sequence(),
            Self::Comment(_) => StatementKind::comment(),
            Self::AlterOwner(_) => StatementKind::alter_owner(),
            Self::CompositeType(_) => StatementKind::composite_type(),
            Self::Domain(_) => StatementKind::domain(),
            Self::Function(_) => StatementKind::function(),
            Self::Schema(_) => StatementKind::schema(),
            Self::Index(_) => StatementKind::index(),
            Self::TableAs(_) => StatementKind::table_as(),
            Self::Table(_) => StatementKind::table(),
            Self::AlterRelation(_) => StatementKind::alter_relation(),
        }
    }

    const fn is_query(&self) -> bool {
        matches!(self, Self::Query(_))
    }

    const fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    const fn is_update(&self) -> bool {
        matches!(self, Self::Update(_))
    }

    const fn is_insert(&self) -> bool {
        matches!(self, Self::Insert(_))
    }

    pub(super) const fn with_node(&self) -> Option<&With> {
        match self {
            Self::Query(node) => Some(&node.with),
            Self::Insert(node) => Some(&node.with),
            Self::Update(node) => Some(&node.with),
            Self::Delete(node) => Some(&node.with),
            _ => None,
        }
    }
}

/// A read-only view of a WITH clause and its ordered CTE collection.
#[derive(Clone, Copy, Debug)]
pub struct WithClause<'a> {
    pub(super) node: &'a With,
    pub(super) dialect: Dialect,
}

impl<'a> WithClause<'a> {
    pub(super) const fn existing(node: &'a With, dialect: Dialect) -> Option<Self> {
        if node.entries.is_empty() {
            None
        } else {
            Some(Self { node, dialect })
        }
    }

    /// Whether WITH RECURSIVE was specified.
    pub const fn is_recursive(&self) -> bool {
        self.node.recursive
    }

    /// Number of CTEs in source order.
    pub const fn len(&self) -> usize {
        self.node.entries.len()
    }

    /// Whether the clause contains no CTEs.
    pub const fn is_empty(&self) -> bool {
        self.node.entries.is_empty()
    }

    /// Inspect CTEs in their source order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CommonTableExpression<'a>> + '_ {
        self.node.entries.iter().map(|node| CommonTableExpression {
            node,
            dialect: self.dialect,
        })
    }

    /// Find a CTE using its parsed identifier spelling.
    ///
    /// PostgreSQL spelling is already normalized by the parser. SQLite compares
    /// ASCII letters without regard to case, matching its identifier behavior.
    pub fn find(&self, name: &str) -> Option<CommonTableExpression<'a>> {
        self.node
            .entries
            .iter()
            .find(|node| self.dialect.same_identifier(&node.name, name))
            .map(|node| CommonTableExpression {
                node,
                dialect: self.dialect,
            })
    }
}

impl Dialect {
    pub(super) fn same_identifier(self, left: &str, right: &str) -> bool {
        if self == Self::sqlite() {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    }
}

/// A read-only CTE, retaining its declaration attributes and body.
#[derive(Clone, Copy, Debug)]
pub struct CommonTableExpression<'a> {
    pub(super) node: &'a Cte,
    pub(super) dialect: Dialect,
}

impl<'a> CommonTableExpression<'a> {
    /// Parsed CTE identifier spelling.
    pub fn name(&self) -> &'a str {
        &self.node.name
    }

    /// Declared column aliases in their original order.
    pub fn columns(&self) -> impl ExactSizeIterator<Item = &'a str> + '_ {
        self.node.columns.iter().map(|column| column.name.as_str())
    }

    /// Explicit materialization choice, or None when unspecified.
    pub const fn materialized(&self) -> Option<bool> {
        self.node.materialized
    }

    /// Whether a SEARCH clause is retained.
    pub const fn has_search(&self) -> bool {
        self.node.search.is_some()
    }

    /// Whether a CYCLE clause is retained.
    pub const fn has_cycle(&self) -> bool {
        self.node.cycle.is_some()
    }

    /// Kind of statement used as the CTE body.
    pub const fn body_kind(&self) -> StatementKind {
        self.node.body.kind()
    }

    /// Inspect a WITH clause belonging to the CTE body itself.
    pub const fn with_clause(&self) -> Option<WithClause<'a>> {
        WithClause::existing(self.node.body.with_node(), self.dialect)
    }
}

impl CteBody {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::Merge(_) => StatementKind::merge(),
        }
    }

    pub(super) const fn with_node(&self) -> &With {
        match self {
            Self::Query(node) => &node.with,
            Self::Insert(node) => &node.with,
            Self::Update(node) => &node.with,
            Self::Delete(node) => &node.with,
            Self::Merge(node) => node.with_node(),
        }
    }
}
