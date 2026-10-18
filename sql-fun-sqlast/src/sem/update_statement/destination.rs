use crate::sem::{
    AnalysisError, ColumnDefinition, ColumnName, ParseContext, TableName, ViewLike,
    ViewMergeSupport, ViewUpdateSupport,
};

/// UPDATE-owned projection of existing relation definitions.
pub(crate) struct UpdateDestination {
    name: TableName,
    columns: Vec<ColumnDefinition>,
    view_support: Option<ViewUpdateSupport>,
    merge_view_support: Option<ViewMergeSupport>,
}
impl UpdateDestination {
    /// Rule definition analysis binds the destination shape without evaluating whether a future write can execute.
    pub(crate) fn resolve_rule_target(
        context: &impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        if let Some(destination) = crate::sem::search_path::expand_search_path_inst(
            context,
            requested.full_name(),
            |key| {
                if context.get_table_impl(key).is_some() {
                    return Some(
                        Self::resolve(context, &TableName::from(key.clone()))
                            .map(|value| value.expect("resolved table")),
                    );
                }
                context.get_view_impl(key).map(|view| {
                    Ok(Self {
                        name: TableName::from(key.clone()),
                        columns: view.result_columns().column_definitions()?,
                        view_support: None,
                        merge_view_support: None,
                    })
                })
            },
        ) {
            destination.map(Some)
        } else {
            Self::resolve(context, requested)
        }
    }
    pub(crate) fn resolve(
        context: &impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        crate::sem::search_path::expand_search_path_inst(context, requested.full_name(), |key| {
            let canonical;
            let key = if context.dialect() == crate::ast::Dialect::sqlite() {
                canonical = context.sqlite_relation_identity(key)?;
                &canonical
            } else {
                key
            };

            if let Some(table) = context.get_table_impl(key) {
                let mut columns = Vec::new();
                while let Some(column) = table.get_column_at(columns.len()) {
                    columns.push(column.clone());
                }
                return Some(Ok(Self {
                    name: TableName::from(key.clone()),
                    columns,
                    view_support: None,
                    merge_view_support: None,
                }));
            }
            context.get_view_impl(key).map(|view| {
                let (columns, support) = if view.is_materialized() {
                    (Vec::new(), ViewUpdateSupport::read_only())
                } else {
                    (
                        view.result_columns().column_definitions()?,
                        ViewUpdateSupport::analyze(context, key)
                            .map_err(|problem| AnalysisError::view_metadata(&problem))?,
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
                    && let Some(definition) = support.update_column_definition(name)
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
            view_support: None,
            merge_view_support: Some(support.clone()),
        })
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
        if let Some(support) = &self.merge_view_support {
            support.supports_update()
        } else {
            self.view_support
                .as_ref()
                .is_none_or(ViewUpdateSupport::is_writable)
        }
    }
    pub(crate) fn can_assign(&self, column: &ColumnName) -> bool {
        if let Some(support) = &self.merge_view_support {
            support.update_column_definition(column).is_some()
        } else {
            self.view_support
                .as_ref()
                .is_none_or(|support| support.can_assign(column))
        }
    }
}
