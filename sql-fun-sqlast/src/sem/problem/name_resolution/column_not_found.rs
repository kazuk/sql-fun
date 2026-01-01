use crate::sem::{
    AnalysisProblem, ColumnName, CteName, TableName, ViewName, data_source::AliasName,
};

use super::NameResolution;

impl NameResolution {
    pub fn column_not_found_in_view(view_name: &ViewName, column: &ColumnName) -> Self {
        Self::ColumnNotFoundInView(view_name.clone(), column.clone())
    }

    pub fn column_not_found_in_cte(cte_name: &CteName, column: &ColumnName) -> Self {
        Self::ColumnNotFoundInCte(cte_name.clone(), column.clone())
    }

    pub fn column_not_found_in_table(table_name: &TableName, column: &ColumnName) -> Self {
        Self::ColumnNotFoundInTable(table_name.clone(), column.clone())
    }

    pub fn column_not_found_in_subquery(alias_name: &AliasName, column: &ColumnName) -> Self {
        Self::ColumnNotFoundInSubQuery(alias_name.clone(), column.clone())
    }

    pub fn column_not_found_in_source(column: &ColumnName) -> Self {
        Self::ColumnNotFoundInSource(column.clone())
    }

    /// column not found in table function
    pub fn column_not_found_in_table_function(alias: &AliasName, column: &ColumnName) -> Self {
        Self::ColumnNotFoundInTableFunction(alias.clone(), column.clone())
    }
}

impl AnalysisProblem {
    /// column not found in view
    #[must_use]
    pub fn column_not_found_in_view(view_name: &ViewName, column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_view(view_name, column))
    }

    /// specfied column not found in sub query
    #[tracing::instrument()]
    pub fn column_not_found_in_subquery(alias: &AliasName, column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_subquery(alias, column))
    }

    /// specified column not found in table
    #[tracing::instrument()]
    pub fn column_not_found_in_table(table_name: &TableName, col_name: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_table(
            table_name, col_name,
        ))
    }

    /// specified column not found in CTE
    #[tracing::instrument()]
    pub fn column_not_found_in_cte(cte_name: &CteName, column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_cte(cte_name, column))
    }

    /// column not found
    #[tracing::instrument()]
    pub fn column_not_found_in_source(column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_source(column))
    }

    /// column not found in table function
    #[tracing::instrument()]
    pub fn column_not_found_in_table_function(alias: &AliasName, column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_table_function(
            alias, column,
        ))
    }
}
