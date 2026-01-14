use crate::{
    TrieMap,
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, BaseParseContext, CreateTable, FullName,
        ImplicitChange, TableInfoRead, TableInfoWrite, TableName,
    },
};

use super::SchemaFileContext;

#[derive(Debug, Default)]
pub struct TableDefinitionCollection(TrieMap<CreateTable>);

impl TableDefinitionCollection {
    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.0.insert(&name.to_string(), create_table.clone());
    }

    fn get(&self, name: &str) -> Option<&CreateTable> {
        self.0.get(name)
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
}

impl<TBaseContext> TableInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        if let Some(v) = self.tables.get(&key.to_string()) {
            Some(v)
        } else {
            self.base.get_table_impl(key)
        }
    }
}

impl<TBaseContext> TableInfoWrite for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.tables.insert_create_table(name, create_table);
    }

    #[tracing::instrument(skip(self))]
    fn apply_alter_table(mut self, alter_table: &AlterTable) -> Result<Self, AnalysisError> {
        let table_name = alter_table.table();
        let Some(table) = self.get_table(table_name).cloned() else {
            self.report_problem(AnalysisProblem::table_not_found(table_name))?;
            return Ok(self);
        };

        let implicit_changes = self.tables.apply_alter(alter_table, table_name, table)?;
        let mut context = self;
        for implicit_change in implicit_changes {
            context = context.apply_implicit_change(implicit_change)?;
        }
        Ok(context)
    }
}
