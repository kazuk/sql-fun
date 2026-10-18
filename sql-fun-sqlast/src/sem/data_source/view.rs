use crate::sem::{AliasSpec, ColumnName, DataSource, Nullable, ViewName, data_source::AliasName};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewDataSource {
    view: ViewName,
    alias: AliasSpec,
}

impl ViewDataSource {
    fn result_column<'a>(
        &self,
        view: &'a crate::sem::ViewLike,
        column: &ColumnName,
    ) -> Option<&'a crate::sem::ResultColumn> {
        let columns = view.result_columns();
        if let Some(index) = self.alias.derived_column_index(column) {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, self.alias.len_derived_column())
        }
    }
    fn column_definition(
        &self,
        view: &crate::sem::ViewLike,
        column: &ColumnName,
    ) -> Option<crate::sem::ColumnDefinition> {
        self.result_column(view, column)
            .map(crate::sem::ResultColumn::create_column_definition)
    }
    pub(super) fn create_column_ref(
        &self,
        context: &mut impl crate::sem::ParseContext,
        alias: &AliasName,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<crate::sem::ColumnReferenceExpr, crate::sem::AnalysisError> {
        let definition = match context.get_view(&self.view) {
            Some(view) => self.column_definition(view, column),
            None => None,
        };
        if definition.is_none() {
            context.report_analysis_problem(
                crate::sem::AnalysisProblem::column_not_found_in_view(&self.view, column),
            )?;
        }
        Ok(crate::sem::ColumnReferenceExpr::view_column(
            alias,
            column,
            &self.view,
            definition.as_ref(),
            nullability,
        ))
    }

    pub(super) fn from_name(view: &ViewName, alias: &AliasSpec) -> Self {
        Self {
            view: view.clone(),
            alias: alias.clone(),
        }
    }

    pub const fn view_name(&self) -> &ViewName {
        &self.view
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
        TParseContext: crate::sem::ParseContext,
    {
        if let Some(view) = context.get_view(&self.view)
            && self.result_column(view, column).is_some()
        {
            names.push(self.alias.clone());
        }
    }
}

impl ViewDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        visitor.relation_identity(self.view.full_name());
        visitor.relation(None, self.alias.name().as_str(), &[]);
        Ok(())
    }
}
