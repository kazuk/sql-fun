use crate::sem::{AliasName, AnalysisError, ColumnName, DataSource, ParseContext, WithClause};

impl DataSource {
    pub(crate) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<(AliasName, ColumnName)>, AnalysisError> {
        if let Self::JoinExpr(join) = self
            && join.matches_projection_qualifier(qualifier)
        {
            return join.projection_names(context, with);
        }
        if qualifier.is_none()
            && let Self::Group(group) = self
        {
            return group.projection_names(context, with);
        }
        let mut columns = Vec::new();
        for alias in self.get_aliases() {
            if qualifier.is_some_and(|qualifier| qualifier != alias.name()) {
                continue;
            }
            let Some((source, _)) = self.resolve_alias(alias.name()) else {
                continue;
            };
            let names = source.column_names(context, with)?;
            for (index, name) in names.into_iter().enumerate() {
                let name = alias.derived_column_at(index).cloned().unwrap_or(name);
                columns.push((alias.name().clone(), name));
            }
        }
        Ok(columns)
    }
    fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        match self {
            Self::Table(table) => {
                let Some(table) = context.get_table(table.name()) else {
                    return AnalysisError::raise_unexpected_input("star relation missing");
                };
                let mut names = Vec::new();
                let mut index = 0;
                while let Some(column) = table.get_column_at(index) {
                    index += 1;
                    if let Some(name) = column.name() {
                        names.push(name.clone());
                    }
                }
                Ok(names)
            }
            Self::CteQuery(cte) => {
                let Some(cte) = with.get(cte.name()) else {
                    return AnalysisError::raise_unexpected_none("star CTE");
                };
                let Some(columns) = cte.result_columns() else {
                    return AnalysisError::raise_unexpected_none("star CTE columns");
                };
                Ok(columns.names())
            }
            Self::View(view) => {
                let Some(view) = context.get_view(view.view_name()) else {
                    return AnalysisError::raise_unexpected_none("star view");
                };
                Ok(view.result_columns().names())
            }
            Self::Group(group) => Ok(group
                .projection_names(context, with)?
                .into_iter()
                .map(|(_, name)| name)
                .collect()),
            Self::SubQuery(query) => Ok(query.select_statement().result_columns().names()),
            _ => AnalysisError::raise_unexpected_input(
                "star source has no enumerable result columns",
            ),
        }
    }
}
