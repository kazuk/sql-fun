use crate::sem::{
    AnalysisError, ColumnDefinition, ColumnName, ParseContext, TableName, ViewUpdateSupport,
};

/// UPDATE-owned projection of existing relation definitions.
pub(crate) struct UpdateDestination {
    name: TableName,
    columns: Vec<ColumnDefinition>,
    view_support: Option<ViewUpdateSupport>,
}
impl UpdateDestination {
    pub(crate) fn resolve(
        context: &impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        crate::sem::search_path::expand_search_path_inst(context, requested.full_name(), |key| {
            if let Some(table) = context.get_table_impl(key) {
                let mut columns = Vec::new();
                while let Some(column) = table.get_column_at(columns.len()) {
                    columns.push(column.clone());
                }
                return Some(Ok(Self {
                    name: TableName::from(key.clone()),
                    columns,
                    view_support: None,
                }));
            }
            context.get_view_impl(key).map(|view| {
                let (columns, support) = if view.is_materialized() {
                    (Vec::new(), ViewUpdateSupport::read_only())
                } else {
                    (
                        view.result_columns().column_definitions()?,
                        context.view_update_support(key),
                    )
                };
                let columns = columns
                    .into_iter()
                    .map(|column| {
                        if let Some(name) = column.name()
                            && let Some(definition) = support.column_definition(name)
                        {
                            definition.clone()
                        } else {
                            column
                        }
                    })
                    .collect();
                Ok(Self {
                    name: TableName::from(key.clone()),
                    columns,
                    view_support: Some(support),
                })
            })
        })
        .transpose()
    }
    pub(crate) const fn name(&self) -> &TableName {
        &self.name
    }
    pub(crate) fn column(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        self.columns
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }
    pub(crate) const fn view_support(&self) -> Option<&ViewUpdateSupport> {
        self.view_support.as_ref()
    }
    pub(crate) fn is_writable(&self) -> bool {
        self.view_support
            .as_ref()
            .is_none_or(ViewUpdateSupport::is_writable)
    }
    pub(crate) fn can_assign(&self, column: &ColumnName) -> bool {
        self.view_support
            .as_ref()
            .is_none_or(|support| support.can_assign(column))
    }
}
