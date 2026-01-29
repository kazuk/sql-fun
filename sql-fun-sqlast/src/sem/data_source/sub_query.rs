use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, Nullable, ParseContext,
    SelectStatement, WithClause,
    data_source::AliasName,
    scalar_expr::{ColumnReferenceExpr, SubQueryColumnRef},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubQueryDataSource {
    select_statement: SelectStatement,
    alias: AliasSpec,
}

impl SubQueryDataSource {
    pub(super) fn new_data_source(s: &SelectStatement, alias: &AliasSpec) -> DataSource {
        DataSource::SubQuery(Box::new(Self {
            select_statement: s.clone(),
            alias: alias.clone(),
        }))
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
        _context: &TParseContext,
        _with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        if self.select_statement.has_column(column) {
            names.push(self.alias.clone());
        }
    }

    pub fn select_statement(&self) -> &SelectStatement {
        &self.select_statement
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
        let result_columns = self.select_statement.result_columns();
        let column_def = if let Some(index) = self.alias.derived_column_index(column) {
            if let Some(result_column_ref) = result_columns.get_column_at(index) {
                let column_def = result_column_ref.create_column_definition();
                Some(column_def)
            } else {
                context.report_problem(
                    AnalysisProblem::subquery_not_have_column_for_derived_column_name(),
                )?;
                None
            }
        } else if let Some(result_column_ref) =
            result_columns.get_column_with_skip_count(column, self.alias.len_derived_column())
        {
            let column_def = result_column_ref.create_column_definition();
            Some(column_def)
        } else {
            context.report_problem(AnalysisProblem::column_not_found_in_subquery(
                self.alias.name(),
                column,
            ))?;
            None
        };

        Ok(ColumnReferenceExpr::SubQueryColumn(SubQueryColumnRef::new(
            self.alias.name(),
            column,
            column_def.as_ref(),
            nullability,
        )))
    }
}
