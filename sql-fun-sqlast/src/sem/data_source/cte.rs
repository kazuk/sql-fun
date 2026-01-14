use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, CteName, DataSource, Nullable,
    ParseContext, WithClause,
    data_source::AliasName,
    scalar_expr::{ColumnReferenceExpr, CteColumnRef},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CteDataSource {
    name: CteName,
    alias: AliasSpec,
}

impl CteDataSource {
    pub(super) fn new_data_source(name: &CteName, alias: &AliasSpec) -> DataSource {
        DataSource::CteQuery(Self {
            name: name.clone(),
            alias: alias.clone(),
        })
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

    pub(super) fn get_aliases_for_column(
        &self,
        column: &ColumnName,
        with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) {
        if let Some(cte) = with_clause.get(&self.name)
            && cte.has_column(column)
        {
            names.push(self.alias.clone());
        }
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub fn name(&self) -> &CteName {
        &self.name
    }

    pub(super) fn create_column_ref<TParseContext>(
        &self,
        context: &mut TParseContext,
        with_clause: &WithClause,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<ColumnReferenceExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(cte) = with_clause.get(&self.name) else {
            AnalysisError::raise_unexpected_input(&format!(
                "cte {} not found in with clause",
                self.name
            ))?;
            AnalysisError::never()
        };

        let column_def = if let Some(index) = self.alias.derived_column_index(column) {
            if let Some(column_def) = cte.get_column_at(context, index) {
                Some(column_def)
            } else {
                context.report_problem(
                    AnalysisProblem::cte_not_have_column_for_derived_column_name(),
                )?;
                None
            }
        } else if let Some(column_def) =
            cte.get_column_def_with_skip_count(context, column, self.alias.len_derived_column())
        {
            Some(column_def)
        } else {
            context.report_problem(AnalysisProblem::column_not_found_in_cte(&self.name, column))?;
            None
        };
        Ok(ColumnReferenceExpr::CteColumn(CteColumnRef::new(
            self.alias.name(),
            &self.name,
            column,
            column_def,
            nullability,
        )))
    }
}
