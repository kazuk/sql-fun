use crate::sem::{ColumnDefinition, ColumnName, FullName};

/// Analyzed capability for UPDATE into an ordinary view.
/// Capabilities are independent of INSERT permissions.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewUpdateSupport {
    writable_columns: Option<Vec<ColumnDefinition>>,
    returning: bool,
    #[serde(default)]
    unassignable_defaults: Vec<ColumnName>,
    #[serde(default)]
    assignment_columns: Vec<crate::sem::ViewAssignmentColumn>,
    #[serde(default)]
    required_trigger_columns: Option<Vec<ColumnName>>,
}
impl ViewUpdateSupport {
    /// Analyze semantic definitions for an exactly bound ordinary view.
    ///
    /// # Errors
    /// Returns the reported structured cause or a diagnostic delivery failure.
    pub fn analyze(
        context: &mut impl crate::sem::ParseContext,
        view: &FullName,
    ) -> Result<Self, crate::sem::AnalysisError> {
        crate::sem::ViewWriteEvaluation::new(context).update(view)
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

    /// Record a column whose DEFAULT is expanded by view rewriting before
    /// the underlying generated/identity assignment check.
    pub fn forbid_default(&mut self, column: &ColumnName) {
        if !self.unassignable_defaults.contains(column) {
            self.unassignable_defaults.push(column.clone());
        }
    }
    /// Whether DEFAULT can be assigned to this view-visible column.
    pub fn can_assign_default(&self, column: &ColumnName) -> bool {
        self.can_assign(column) && !self.unassignable_defaults.contains(column)
    }

    /// The view cannot accept UPDATE statements.
    pub const fn read_only() -> Self {
        Self {
            writable_columns: None,
            returning: false,
            unassignable_defaults: Vec::new(),
            assignment_columns: Vec::new(),
            required_trigger_columns: None,
        }
    }
    /// The view accepts UPDATE, with exactly these assignable columns.
    /// Analysis accounts for automatic view updates, INSTEAD OF UPDATE triggers,
    /// and applicable INSTEAD rules according to the selected dialect.
    /// Definitions retain view-visible names and assignment types, plus effective
    /// default, generated and identity constraints needed for UPDATE validation.
    pub fn writable(columns: &[ColumnDefinition], returning: bool) -> Self {
        Self {
            writable_columns: Some(columns.to_vec()),
            returning,
            unassignable_defaults: Vec::new(),
            assignment_columns: Vec::new(),
            required_trigger_columns: None,
        }
    }
    /// Restrict SQLite UPDATE to SET lists activating at least one trigger column.
    /// None records a trigger without an UPDATE OF restriction.
    pub fn set_required_trigger_columns(&mut self, columns: Option<&[ColumnName]>) {
        self.required_trigger_columns = columns.map(<[ColumnName]>::to_vec);
    }
    /// Whether the complete SET target list activates a matching SQLite
    /// INSTEAD OF UPDATE trigger. UPDATE OF requires at least one listed column.
    pub fn accepts_assignments(&self, columns: &[ColumnName]) -> bool {
        let Some(required) = &self.required_trigger_columns else {
            return true;
        };
        for column in required {
            for assigned in columns {
                if column.as_str().eq_ignore_ascii_case(assigned.as_str()) {
                    return true;
                }
            }
        }
        false
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
    /// Retrieve explicitly registered support by exact identity, without deriving
    /// capabilities or interpreting absence. The analyzer owns those decisions.
    fn view_update_support(&self, view: &FullName) -> Option<&ViewUpdateSupport>;
}
