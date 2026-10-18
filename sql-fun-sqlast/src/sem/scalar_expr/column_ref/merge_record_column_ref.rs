use crate::sem::{
    ColumnDefinition, ColumnName, TypeReference,
    data_source::MergeRecordRole,
    scalar_expr::column_ref::{ColumnReference, ColumnReferenceExpr},
};

/// A MERGE record column that retains its resolved relation binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeRecordColumnRef {
    role: MergeRecordRole,
    source: Box<ColumnReferenceExpr>,
    nullable: bool,
}

impl MergeRecordColumnRef {
    pub(crate) fn new(role: MergeRecordRole, source: &ColumnReferenceExpr, nullable: bool) -> Self {
        Self {
            role,
            source: Box::new(source.clone()),
            nullable,
        }
    }

    /// The MERGE record represented by this reference.
    pub const fn role(&self) -> MergeRecordRole {
        self.role
    }

    /// The completed relation-column lookup, including any missing-column outcome.
    pub fn source(&self) -> &ColumnReferenceExpr {
        &self.source
    }

    pub(crate) fn column_type(&self) -> Option<TypeReference> {
        self.source.get_type()
    }

    pub(crate) fn is_not_null(&self) -> Option<bool> {
        if self.nullable {
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

impl ColumnReference for MergeRecordColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        self.column_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        self.column_definition()
    }

    fn get_column_name(&self) -> &ColumnName {
        self.column_name()
    }
}
