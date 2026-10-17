use crate::sem::{ColumnName, Constraint, FullName, TableName};

/// The completed lookup outcome for an explicit ON CONSTRAINT reference.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertConstraintKind {
    /// A primary-key constraint was found.
    PrimaryKey,
    /// A unique constraint was found.
    Unique,
    /// A constraint was found but cannot arbitrate this action.
    WrongKind,
    /// No constraint with that name belongs to the destination.
    NotFound,
}

impl InsertConstraintKind {
    const fn primary_key() -> Self {
        Self::PrimaryKey
    }
    const fn unique() -> Self {
        Self::Unique
    }
    const fn wrong_kind() -> Self {
        Self::WrongKind
    }
    const fn not_found() -> Self {
        Self::NotFound
    }
    const fn is_usable(self) -> bool {
        matches!(self, Self::PrimaryKey | Self::Unique)
    }
}

impl Constraint {
    pub(crate) const fn insert_constraint_kind(&self) -> InsertConstraintKind {
        match self {
            Self::PrimaryKey(_) => InsertConstraintKind::primary_key(),
            Self::Unique(_) => InsertConstraintKind::unique(),
            Self::Foreign(_) => InsertConstraintKind::wrong_kind(),
        }
    }
}

/// A destination-qualified constraint reference and its completed lookup.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertConstraintLookup {
    table: TableName,
    name: String,
    kind: InsertConstraintKind,
}
impl InsertConstraintLookup {
    pub(crate) fn new(table: &TableName, name: &str, constraint: Option<&Constraint>) -> Self {
        Self {
            table: table.clone(),
            name: name.to_owned(),
            kind: constraint.map_or(
                InsertConstraintKind::not_found(),
                Constraint::insert_constraint_kind,
            ),
        }
    }
    /// The destination whose constraint namespace was searched.
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// The explicitly supplied constraint name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The kind found, or the explicit lookup failure.
    pub const fn kind(&self) -> InsertConstraintKind {
        self.kind
    }
    pub(crate) const fn is_usable(&self) -> bool {
        self.kind.is_usable()
    }
}

/// One catalog definition inferred as an INSERT conflict arbiter.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertArbiter {
    /// An index identified by its qualified catalog name.
    Index(FullName),
    /// An arbiter reached through a view rewrite, with its actual relation.
    ViewSource(Box<ViewSourceArbiter>),
    /// A named table constraint, qualified by the INSERT destination.
    TableConstraint(InsertConstraintLookup),
    /// An inline unique/primary-key constraint on a bound destination column.
    ColumnConstraint(ColumnName),
    /// An unnamed table key, identified by its bound destination columns.
    AnonymousTableKey(Vec<ColumnName>),
}
/// Qualified source retained when rewriting a view conflict target.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewSourceArbiter {
    relation: TableName,
    arbiter: InsertArbiter,
}
impl InsertArbiter {
    pub(crate) fn with_view_source(self, relation: &TableName) -> Self {
        if self.source_relation().is_some() {
            return self;
        }
        Self::ViewSource(Box::new(ViewSourceArbiter {
            relation: relation.clone(),
            arbiter: self,
        }))
    }
    /// Actual destination of a view-mapped arbiter; never the view identity.
    pub fn source_relation(&self) -> Option<&TableName> {
        if let Self::ViewSource(source) = self {
            Some(&source.relation)
        } else {
            None
        }
    }

    /// Bind an eligible index by its catalog identity.
    pub fn index(name: &FullName) -> Self {
        Self::Index(name.clone())
    }
    /// Bind an eligible named or unnamed table key.
    pub fn table_constraint(table: &TableName, constraint: &Constraint) -> Self {
        if constraint.name().is_empty() {
            Self::AnonymousTableKey(constraint.unique_key_columns())
        } else {
            Self::TableConstraint(InsertConstraintLookup::new(
                table,
                constraint.name(),
                Some(constraint),
            ))
        }
    }
    /// Bind an eligible inline key on a destination column.
    pub fn column_constraint(column: &ColumnName) -> Self {
        Self::ColumnConstraint(column.clone())
    }
    /// The bound columns of an unnamed table key, if applicable.
    pub fn anonymous_key_columns(&self) -> Option<&[ColumnName]> {
        if let Self::ViewSource(source) = self {
            return source.arbiter.anonymous_key_columns();
        }
        if let Self::AnonymousTableKey(columns) = self {
            Some(columns)
        } else {
            None
        }
    }
    /// The qualified index, if this arbiter is an index.
    pub fn index_name(&self) -> Option<&FullName> {
        if let Self::ViewSource(source) = self {
            return source.arbiter.index_name();
        }
        if let Self::Index(name) = self {
            Some(name)
        } else {
            None
        }
    }
    /// The bound table constraint, if applicable.
    pub fn constraint(&self) -> Option<&InsertConstraintLookup> {
        if let Self::ViewSource(source) = self {
            return source.arbiter.constraint();
        }
        if let Self::TableConstraint(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// The bound inline-key column, if applicable.
    pub fn column(&self) -> Option<&ColumnName> {
        if let Self::ViewSource(source) = self {
            return source.arbiter.column();
        }
        if let Self::ColumnConstraint(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

/// Completed conflict-target lookup, distinct from retained inference expressions.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertArbiterLookup {
    /// The SQL omits a target and handles all usable conflicts.
    Untargeted,
    /// Lookup of a named destination constraint.
    Constraint(InsertConstraintLookup),
    /// Definitions matching the inference target.
    Inferred(Vec<InsertArbiter>),
    /// The supplied inference target matched no eligible catalog definition.
    NotFound,
}
impl InsertArbiterLookup {
    pub(super) const fn initial(targeted: bool) -> Self {
        if targeted {
            Self::NotFound
        } else {
            Self::Untargeted
        }
    }
    pub(crate) fn named(value: &InsertConstraintLookup) -> Self {
        Self::Constraint(value.clone())
    }
    pub(crate) fn inferred(values: &[InsertArbiter]) -> Self {
        if values.is_empty() {
            Self::NotFound
        } else {
            Self::Inferred(values.to_vec())
        }
    }
    /// The completed named-constraint lookup, if supplied.
    pub const fn constraint(&self) -> Option<&InsertConstraintLookup> {
        if let Self::Constraint(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// The inferred definitions; absent for untargeted or unsuccessful lookup.
    pub fn arbiters(&self) -> Option<&[InsertArbiter]> {
        if let Self::Inferred(values) = self {
            Some(values)
        } else {
            None
        }
    }
    /// Whether an inference target had no match.
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    /// Whether no target was supplied in the SQL.
    pub const fn is_untargeted(&self) -> bool {
        matches!(self, Self::Untargeted)
    }
}
