use crate::sem::{ColumnDefinition, ColumnName, ColumnReferenceExpr, TypeReference};

/// Row image bound by a PostgreSQL 18 DELETE RETURNING reference.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DeleteRowVersion {
    /// Values before deletion.
    Old,
    /// Values after deletion (nullable for DELETE).
    New,
}
impl DeleteRowVersion {
    /// Select the values before deletion.
    pub const fn old() -> Self {
        Self::Old
    }
    /// Select the nullable values after deletion.
    pub const fn new_image() -> Self {
        Self::New
    }
    const fn is_new(&self) -> bool {
        matches!(self, Self::New)
    }

    pub(crate) fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "old" => Some(Self::Old),
            "new" => Some(Self::New),
            _ => None,
        }
    }
}

/// A resolved DELETE row-image column, retaining its underlying catalog binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteRowColumn {
    version: DeleteRowVersion,
    source: Box<ColumnReferenceExpr>,
}
impl DeleteRowColumn {
    pub(crate) fn new(version: DeleteRowVersion, source: &ColumnReferenceExpr) -> Self {
        Self {
            version,
            source: Box::new(source.clone()),
        }
    }
    /// Row image selected by the reference.
    pub const fn version(&self) -> DeleteRowVersion {
        self.version
    }
    /// Completed target-column lookup, including a missing-column outcome.
    pub fn source(&self) -> &ColumnReferenceExpr {
        &self.source
    }
    pub(crate) fn column_type(&self) -> Option<TypeReference> {
        self.source.get_type()
    }
    pub(crate) fn is_not_null(&self) -> Option<bool> {
        if self.version.is_new() {
            Some(false)
        } else {
            self.source.is_not_null()
        }
    }
    pub(crate) fn column_definition(&self) -> &Option<ColumnDefinition> {
        self.source.get_column_def()
    }
    pub(crate) fn column_name(&self) -> &ColumnName {
        self.source.get_column_name()
    }
}
