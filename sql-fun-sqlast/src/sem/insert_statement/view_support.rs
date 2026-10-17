use crate::sem::{ColumnDefinition, ColumnName, FullName};

/// Catalog decision about INSERT into an ordinary view.
/// An empty writable-column list still permits DEFAULT VALUES.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewInsertSupport {
    #[serde(default)]
    missing_metadata: Option<FullName>,
    writable_columns: Option<Vec<ColumnDefinition>>,
    returning: bool,
    #[serde(default)]
    assignment_columns: Vec<crate::sem::ViewAssignmentColumn>,
    on_conflict: bool,
}
impl ViewInsertSupport {
    pub(crate) fn missing_metadata(view: &FullName) -> Self {
        let mut support = Self::read_only();
        support.missing_metadata = Some(view.clone());
        support
    }
    /// An explicitly incomplete catalog decision, distinct from read-only support.
    pub const fn missing_metadata_view(&self) -> Option<&FullName> {
        self.missing_metadata.as_ref()
    }

    /// Record alias equivalence for automatic-view assignment targets.
    /// Trigger/rule-backed NEW-row fields have independent identities instead.
    pub fn set_assignment_columns(&mut self, columns: &[crate::sem::ViewAssignmentColumn]) {
        self.assignment_columns = columns.to_vec();
    }
    /// The underlying assignment-column key, or the supplied view name when
    /// no rewrite mapping applies. Keys are compared within this destination.
    pub fn assignment_column<'a>(&'a self, name: &'a ColumnName) -> &'a ColumnName {
        self.assignment_columns
            .iter()
            .find(|column| column.name() == name)
            .map_or(name, crate::sem::ViewAssignmentColumn::source_column)
    }

    /// The view cannot accept INSERT statements.
    pub const fn read_only() -> Self {
        Self {
            missing_metadata: None,
            writable_columns: None,
            returning: false,
            assignment_columns: Vec::new(),
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
            missing_metadata: None,
            writable_columns: Some(columns.to_vec()),
            returning,
            assignment_columns: Vec::new(),
            on_conflict,
        }
    }
    /// Whether the view supports INSERT RETURNING (rules may restrict this).
    pub const fn supports_returning(&self) -> bool {
        self.returning
    }
    /// Whether any ON CONFLICT clause is allowed, before arbiter inference.
    /// SQLite views, rule replacement paths, and trigger paths with INSERT/UPDATE
    /// rules reject UPSERT. Additional rules can coexist with automatic writes.
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
    /// Missing loaded metadata is retained explicitly by `missing_metadata_view`,
    /// independently of a completed read-only decision. Unimplemented producer
    /// functionality remains explicit at its implementation boundary.
    /// No trigger or rule body is executed by INSERT name resolution.
    fn view_insert_support(&self, view: &FullName) -> ViewInsertSupport;
}
