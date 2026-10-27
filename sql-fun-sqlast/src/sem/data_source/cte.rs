use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnName, CteName, DataSource, Nullable,
        WithClause, data_source::AliasName, scalar_expr::ColumnReferenceExpr,
    },
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

    pub const fn name(&self) -> &CteName {
        &self.name
    }

    pub(super) fn create_column_ref<TParseContext>(
        &self,
        context: &mut TParseContext,
        with_clause: &WithClause,
        alias: &AliasName,
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

        let index = self.alias.derived_column_index(column);
        let column_def = cte.resolved_column(column, index, self.alias.len_derived_column());
        if column_def.is_none() {
            if index.is_some() {
                context.report_analysis_problem(
                    AnalysisProblem::cte_not_have_column_for_derived_column_name(),
                )?;
            } else {
                context.report_analysis_problem(AnalysisProblem::column_not_found_in_cte(
                    &self.name, column,
                ))?;
            }
        }
        Ok(ColumnReferenceExpr::cte_column(
            alias,
            &self.name,
            column,
            column_def.as_ref(),
            nullability,
        ))
    }
}

impl CteDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        visitor.relation(
            Some(&self.name),
            self.alias.name().as_str(),
            &self.alias.inference_columns(),
        );
        Ok(())
    }
}

impl CteDataSource {
    pub(super) fn projection_column_names(
        &self,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        let Some(cte) = with.get(&self.name) else {
            return AnalysisError::raise_unexpected_none("star CTE");
        };
        let Some(columns) = cte.result_columns() else {
            return AnalysisError::raise_unexpected_none("star CTE columns");
        };
        Ok(columns.names())
    }
}
