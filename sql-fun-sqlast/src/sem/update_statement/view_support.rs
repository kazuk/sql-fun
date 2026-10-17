use crate::sem::{ColumnDefinition, ColumnName, FullName};

/// Completed catalog decision about UPDATE into an ordinary view.
/// Capabilities are independent of INSERT permissions.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewUpdateSupport {
    writable_columns: Option<Vec<ColumnDefinition>>,
    returning: bool,
}
impl ViewUpdateSupport {
    /// The view cannot accept UPDATE statements.
    pub const fn read_only() -> Self {
        Self {
            writable_columns: None,
            returning: false,
        }
    }
    /// The view accepts UPDATE, with exactly these assignable columns.
    /// Catalogs must account for automatic view updates, INSTEAD OF UPDATE triggers,
    /// and applicable INSTEAD rules according to the selected dialect.
    /// Definitions retain view-visible names and assignment types, plus effective
    /// default, generated and identity constraints needed for UPDATE validation.
    pub fn writable(columns: &[ColumnDefinition], returning: bool) -> Self {
        Self {
            writable_columns: Some(columns.to_vec()),
            returning,
        }
    }
    /// Whether the view supports UPDATE RETURNING (rules may restrict this).
    pub const fn supports_returning(&self) -> bool {
        self.returning
    }
    /// Whether UPDATE is supported, at the relation level.
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
pub trait ViewUpdateInfoRead {
    /// Return a completed capability decision for an existing ordinary view.
    /// Missing trigger/rule/update metadata must remain explicitly unimplemented
    /// in the catalog, rather than being interpreted as a read-only view.
    /// No trigger or rule body is executed by UPDATE name resolution.
    fn view_update_support(&self, view: &FullName) -> ViewUpdateSupport;
}
