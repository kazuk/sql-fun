use crate::ast::AlterRelationKind;

impl AlterRelationKind {
    pub(crate) const fn supports_table_column_attributes(self) -> bool {
        matches!(self, Self::Table)
    }
}
