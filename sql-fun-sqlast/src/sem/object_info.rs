//! Shared relation-name collision domain. Definitions remain in their own catalogs.
use crate::sem::FullName;

/// Kind occupying a shared relation name; unrelated namespaces are excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Table.
    Table,
    /// Ordinary view.
    View,
    /// Materialized view.
    MaterializedView,
    /// Sequence.
    Sequence,
    /// PostgreSQL standalone composite type (also backed by a relation).
    CompositeType,
    /// Index.
    Index,
}
impl Kind {
    /// Table kind.
    pub const fn table() -> Self {
        Self::Table
    }
    /// Ordinary view kind.
    pub const fn view() -> Self {
        Self::View
    }
    /// Materialized view kind.
    pub const fn materialized_view() -> Self {
        Self::MaterializedView
    }
    /// Sequence kind.
    pub const fn sequence() -> Self {
        Self::Sequence
    }
    /// PostgreSQL standalone composite type kind.
    pub const fn composite_type() -> Self {
        Self::CompositeType
    }
    /// Index kind.
    pub const fn index() -> Self {
        Self::Index
    }
    pub(crate) const fn creates_row_type(self) -> bool {
        matches!(self, Self::Table | Self::View | Self::MaterializedView)
    }
    pub(crate) const fn permits_sqlite_noop(self, existing: Self) -> bool {
        match self {
            Self::Table | Self::View => matches!(existing, Self::Table | Self::View),
            Self::Index => matches!(existing, Self::Index),
            _ => false,
        }
    }
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::Sequence => "sequence",
            Self::Index => "index",
            Self::CompositeType => "composite type",
        })
    }
}
/// Read the kind at an exact qualified identity, without search-path expansion.
/// Mutation belongs exclusively to definition write/update operations.
pub trait ObjectInfoRead {
    /// Relation kind, including inherited definitions.
    fn object_kind(&self, name: &FullName) -> Option<Kind>;
}
/// Read extension existence in its separate database-wide namespace.
pub trait ExtensionInfoRead {
    /// Whether the current or inherited catalog contains this extension name.
    fn extension_exists(&self, name: &str) -> bool;
}
