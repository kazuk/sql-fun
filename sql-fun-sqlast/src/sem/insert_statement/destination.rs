use crate::sem::{
    AnalysisError, ColumnDefinition, ColumnName, Constraint, CreateTable, ParseContext, TableName,
    ViewInsertSupport, ViewLike, ViewMergeSupport,
};

/// INSERT-owned projection of existing relation definitions.
pub(crate) struct InsertDestination {
    name: TableName,
    columns: Vec<ColumnDefinition>,
    table: Option<CreateTable>,
    view_support: Option<ViewInsertSupport>,
    merge_view_support: Option<ViewMergeSupport>,
}
impl InsertDestination {
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
                    table: Some(table.clone()),
                    view_support: None,
                    merge_view_support: None,
                }));
            }
            context.get_view_impl(key).map(|view| {
                let (columns, support) = if view.is_materialized() {
                    (Vec::new(), ViewInsertSupport::read_only())
                } else {
                    (
                        view.result_columns().column_definitions()?,
                        context.view_insert_support(key),
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
                    table: None,
                    view_support: Some(support),
                    merge_view_support: None,
                })
            })
        })
        .transpose()
    }

    pub(crate) fn for_merge_view(
        name: &TableName,
        view: &ViewLike,
        support: &ViewMergeSupport,
    ) -> Result<Self, AnalysisError> {
        let columns = if view.is_materialized() {
            Vec::new()
        } else {
            view.result_columns().column_definitions()?
        };
        let columns = columns
            .into_iter()
            .map(|column| {
                if let Some(name) = column.name()
                    && let Some(definition) = support.insert_column_definition(name)
                {
                    definition.clone()
                } else {
                    column
                }
            })
            .collect();
        Ok(Self {
            name: name.clone(),
            columns,
            table: None,
            view_support: None,
            merge_view_support: Some(support.clone()),
        })
    }
    pub(crate) const fn name(&self) -> &TableName {
        &self.name
    }
    pub(crate) fn get_column_at(&self, index: usize) -> Option<&ColumnDefinition> {
        self.columns.get(index)
    }
    pub(crate) fn get_column_def(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        self.columns
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }
    pub(crate) fn constraint(&self, name: &str) -> Option<&Constraint> {
        if let Some(table) = &self.table {
            table.constraint(name)
        } else {
            None
        }
    }
    pub(crate) const fn view_support(&self) -> Option<&ViewInsertSupport> {
        self.view_support.as_ref()
    }
    pub(crate) fn is_writable(&self) -> bool {
        if let Some(support) = &self.merge_view_support {
            support.supports_insert()
        } else {
            self.view_support
                .as_ref()
                .is_none_or(ViewInsertSupport::is_writable)
        }
    }
    pub(crate) fn can_assign(&self, column: &ColumnName) -> bool {
        if let Some(support) = &self.merge_view_support {
            support.insert_column_definition(column).is_some()
        } else {
            self.view_support
                .as_ref()
                .is_none_or(|support| support.can_assign(column))
        }
    }
}
