use crate::sem::{ColumnDefinition, ColumnName, FullName};

/// Analyzed capability for INSERT into an ordinary view.
/// An empty writable-column list still permits DEFAULT VALUES.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewInsertSupport {
    writable_columns: Option<Vec<ColumnDefinition>>,
    returning: bool,
    #[serde(default)]
    assignment_columns: Vec<crate::sem::ViewAssignmentColumn>,
    on_conflict: bool,
}
impl ViewInsertSupport {
    /// Analyze semantic definitions for an exactly bound ordinary view.
    ///
    /// # Errors
    /// Returns the reported structured cause or a diagnostic delivery failure.
    pub fn analyze(
        context: &mut impl crate::sem::ParseContext,
        view: &FullName,
    ) -> Result<Self, crate::sem::AnalysisError> {
        crate::sem::ViewWriteEvaluation::new(context).insert(view)
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
            writable_columns: None,
            returning: false,
            assignment_columns: Vec::new(),
            on_conflict: false,
        }
    }
    /// The view accepts INSERT, with exactly these assignable columns.
    /// Analysis accounts for automatic updates, INSTEAD OF INSERT triggers,
    /// and applicable INSTEAD rules according to the selected dialect.
    /// Definitions retain view-visible names and assignment types, plus effective
    /// default, generated and identity constraints needed for INSERT validation.
    pub fn writable(columns: &[ColumnDefinition], returning: bool, on_conflict: bool) -> Self {
        Self {
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
    /// Retrieve explicitly registered support by exact identity, without deriving
    /// capabilities or interpreting absence. The analyzer owns those decisions.
    fn view_insert_support(&self, view: &FullName) -> Option<&ViewInsertSupport>;
}
