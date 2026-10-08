use crate::{
    TrieMap,
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, BaseParseContext, CreateTable,
        DiagnosticContext, DropTable, FullName, ImplicitChange, TableInfoRead, TableInfoWrite,
        TableName,
    },
};

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
pub struct TableDefinitionCollection(TrieMap<CreateTable>);

impl TableDefinitionCollection {
    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.0.insert(&name.to_string(), create_table.clone());
    }

    fn get(&self, name: &FullName) -> Option<&CreateTable> {
        self.0.get(&name.to_string())
    }

    fn get_mut(&mut self, name: &FullName) -> Option<&mut CreateTable> {
        self.0.get_mut(&name.to_string())
    }

    fn apply_alter(
        &mut self,
        alter_table: &AlterTable,
        table_name: &TableName,
        mut table: CreateTable,
    ) -> Result<Vec<ImplicitChange>, AnalysisError> {
        let implicit_changes = table.apply_alter(alter_table)?;
        self.insert_create_table(table_name.full_name(), &table);
        Ok(implicit_changes)
    }

    fn push_drop_statement(
        &mut self,
        table_name: &TableName,
        drop_table: &DropTable,
    ) -> Result<bool, AnalysisError> {
        if let Some(table) = self.get_mut(table_name.full_name()) {
            table.push_drop_statement(drop_table);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl TableInfoRead for SchemaCatalog {
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        if let Some(v) = self.tables.get(key) {
            Some(v)
        } else {
            self.base.get_table_impl(key)
        }
    }
}

impl TableInfoWrite for SchemaFileContext {
    #[tracing::instrument(skip(self))]
    fn apply_alter_table(mut self, alter_table: &AlterTable) -> Result<Self, AnalysisError> {
        let table_name = alter_table.table();
        let Some(table) = self.get_table(table_name).cloned() else {
            self.report_problem(AnalysisProblem::table_not_found(table_name))?;
            return Ok(self);
        };

        let implicit_changes = self
            .catalog
            .tables
            .apply_alter(alter_table, table_name, table)?;
        let mut context = self;
        for implicit_change in implicit_changes {
            context = context.apply_implicit_change(implicit_change)?;
        }
        Ok(context)
    }

    fn apply_drop_table(
        mut self,
        drop_table: &crate::sem::DropTable,
    ) -> Result<Self, AnalysisError> {
        let table_names = drop_table.table_names();
        for table_name in table_names {
            if !self
                .catalog
                .tables
                .push_drop_statement(table_name, drop_table)?
            {
                self.report_problem(AnalysisProblem::table_not_found(table_name))?;
            }
        }
        Ok(self)
    }

    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.catalog.tables.insert_create_table(name, create_table);
    }
}
