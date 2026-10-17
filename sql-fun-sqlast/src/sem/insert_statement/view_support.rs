use crate::sem::{ColumnDefinition, ColumnName, FullName};

/// Completed catalog decision about INSERT into an ordinary view.
/// An empty writable-column list still permits DEFAULT VALUES.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewInsertSupport {
    writable_columns: Option<Vec<ColumnDefinition>>,
    returning: bool,
    on_conflict: bool,
}
impl ViewInsertSupport {
    /// The view cannot accept INSERT statements.
    pub const fn read_only() -> Self {
        Self {
            writable_columns: None,
            returning: false,
            on_conflict: false,
        }
    }
    /// The view accepts INSERT, with exactly these assignable columns.
    /// Catalogs must account for automatic updates, INSTEAD OF INSERT triggers,
    /// and applicable INSTEAD rules according to the selected dialect.
    /// Definitions retain view-visible names and assignment types, plus effective
    /// default, generated and identity constraints needed for INSERT validation.
    pub fn writable(columns: &[ColumnDefinition], returning: bool, on_conflict: bool) -> Self {
        Self {
            writable_columns: Some(columns.to_vec()),
            returning,
            on_conflict,
        }
    }
    /// Whether the view supports INSERT RETURNING (rules may restrict this).
    pub const fn supports_returning(&self) -> bool {
        self.returning
    }
    /// Whether any ON CONFLICT clause is allowed, before arbiter inference.
    /// SQLite views and PostgreSQL views with INSERT/UPDATE rules reject UPSERT.
    pub const fn supports_on_conflict(&self) -> bool {
        self.on_conflict
    }
    /// Whether INSERT is supported, including DEFAULT VALUES.
    pub const fn is_writable(&self) -> bool {
        self.writable_columns.is_some()
    }
    /// Effective assignment metadata for a writable view column.
    pub fn column_definition(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        let columns = self.writable_columns.as_ref()?;
        columns
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }
    /// Whether an explicit or implicit assignment to this column is allowed.
    /// DEFAULT is an assignment too.
    pub fn can_assign(&self, column: &ColumnName) -> bool {
        self.column_definition(column).is_some()
    }
}

/// Catalog service for view destinations. The name is already bound and
/// qualified; this is not a second search-path lookup.
pub trait ViewInsertInfoRead {
    /// Return a completed capability decision for an existing ordinary view.
    /// Missing trigger/rule/update metadata must remain explicitly unimplemented
    /// in the catalog, rather than being interpreted as a read-only view.
    /// No trigger or rule body is executed by INSERT name resolution.
    fn view_insert_support(&self, view: &FullName) -> ViewInsertSupport;
}
