use crate::{
    ast::{ConflictClause, Validity},
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

/// `NULL` / `NOT NULL` constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ColumnNullabilityConstraint {
    table_name: TableName,
    column_name: ColumnName,
    is_not_null: bool,
    conflict_clause: Option<ConflictClause>,
    validity: Option<Validity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    no_inherit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validated: Option<bool>,
}

impl ColumnNullabilityConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        is_not_null: bool,
        conflict_clause: Option<ConflictClause>,
        validity: Option<Validity>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            is_not_null,
            conflict_clause,
            validity,
            name: None,
            no_inherit: false,
            validated: None,
        }
    }

    pub(crate) fn retain_declaration(&mut self, name: Option<&str>, no_inherit: bool) {
        self.name = name.map(str::to_owned);
        self.no_inherit = no_inherit;
    }

    /// SQL constraint name, when known.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Whether this constraint was declared NO INHERIT.
    pub const fn no_inherit(&self) -> bool {
        self.no_inherit
    }

    pub(crate) const fn validate_on_create(&mut self) {
        self.validated = Some(true);
    }

    /// Whether this constraint is validated for existing rows.
    pub fn is_validated(&self) -> bool {
        self.validated.unwrap_or_else(|| {
            self.validity
                .as_ref()
                .is_none_or(|validity| !validity.skips_validation())
        })
    }

    pub(crate) fn accepts_redeclaration(&self, requested: &Self) -> bool {
        self.name == requested.name
            && self.no_inherit == requested.no_inherit
            && (self.is_validated() || !requested.is_validated())
    }

    /// get nullability
    pub const fn is_not_null(&self) -> bool {
        self.is_not_null
    }
}

impl ColumnConstraintTrait for ColumnNullabilityConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Nullability
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Nullability variant
    pub const fn nullability(value: ColumnNullabilityConstraint) -> Self {
        Self::Nullability(value)
    }

    /// get Nullability variant
    pub const fn as_nullability(&self) -> Option<&ColumnNullabilityConstraint> {
        if let Self::Nullability(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl ColumnNullabilityConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        not_null: bool,
        conflict: &Option<ConflictClause>,
        validity: &Option<Validity>,
    ) -> Self {
        Self::new(table, column, not_null, conflict.clone(), validity.clone())
    }
}
