//! Prepare every dependent edit before publishing any catalog changes.

use crate::{
    context::{SchemaFileContext, TableInfoRead, schema_file_context::table_rename::RenameUpdates},
    sem::{AlterTable, AnalysisError, ColumnRename},
};
impl SchemaFileContext {
    pub(super) fn apply_column_rename(
        mut self,
        alteration: &AlterTable,
        rename: &ColumnRename,
    ) -> Result<Self, AnalysisError> {
        let Some(target) = alteration.resolved_target() else {
            return AnalysisError::raise_unexpected_none("applicable column rename target");
        };
        let Some(mut table) = self.get_table_impl(target.full_name()).cloned() else {
            return AnalysisError::raise_unexpected_none("bound column rename table");
        };
        table.apply_alter(alteration)?;
        table.rename_column_dependencies(rename, true)?;
        let updates = RenameUpdates::prepare_column(&self, rename)?;
        self.catalog.store_table(target.full_name(), &table);
        updates.commit(&mut self.catalog, target);
        Ok(self)
    }
}
