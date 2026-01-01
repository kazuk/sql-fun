use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, Nullable, ParseContext,
    TableName,
    data_source::AliasName,
    scalar_expr::{ColumnReferenceExpr, TableColumnRef},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableDataSource {
    name: TableName,
    alias: AliasSpec,
}

impl TableDataSource {
    pub(super) fn new_data_source(name: &TableName, alias: &AliasSpec) -> DataSource {
        DataSource::Table(Self {
            name: name.clone(),
            alias: alias.clone(),
        })
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        if let Some(table) = context.get_table(&self.name)
            && table.has_column(column)
        {
            names.push(self.alias.clone());
        }
    }

    pub fn name(&self) -> &TableName {
        &self.name
    }

    pub(super) fn create_column_ref<TParseContext>(
        &self,
        context: &mut TParseContext,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<ColumnReferenceExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(table) = context.get_table(&self.name) else {
            AnalysisError::raise_unexpected_input(&format!(
                "table {} not found in context",
                self.name
            ))?;
            AnalysisError::never()
        };

        let column_def = if let Some(index) = self.alias.derived_column_index(column) {
            if let Some(column_def) = table.get_column_at(index) {
                Some(column_def)
            } else {
                context.report_problem(
                    AnalysisProblem::table_not_have_column_for_derived_column_name(),
                );
                None
            }
        } else if let Some(column_def) =
            table.get_column_def_with_skip_count(column, self.alias.len_derived_column())
        {
            Some(column_def)
        } else {
            context.report_problem(AnalysisProblem::column_not_found_in_table(
                &self.name, column,
            ));
            None
        };
        Ok(ColumnReferenceExpr::TableColumn(TableColumnRef::new(
            self.alias.name(),
            column,
            &self.name,
            column_def,
            nullability,
        )))
    }
}
