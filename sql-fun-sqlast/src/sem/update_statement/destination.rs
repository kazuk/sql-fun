use crate::sem::search_path::expand_relation_search_path;
use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnDefinition, ParseContext, ViewLike, ViewMergeSupport,
        ViewUpdateSupport,
    },
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
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        let Some(name) = context.resolve_relation_name(requested.full_name()) else {
            return Self::resolve(context, requested);
        };
        let name = TableName::from(name);
        if context.get_table(&name).is_some() {
            return Self::resolve(context, &name);
        }
        let view = context
            .get_view(&crate::names::ViewName::from_full_name(name.full_name()))
            .expect("resolved view");
        Self::from_rule_view(&name, view).map(Some)
    }
    fn from_rule_view(name: &TableName, view: &ViewLike) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: name.clone(),
            columns: view.result_columns().column_definitions()?,
            view_support: None,
            merge_view_support: None,
        })
    }
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        let key = expand_relation_search_path(context, requested.full_name(), |key| {
            if context.dialect() == crate::ast::Dialect::sqlite() {
                context.sqlite_relation_identity(key)
            } else {
                context.resolve_relation_name(key)
            }
        });
        let Some(key) = key else {
            return Ok(None);
        };
        Self::resolve_at(context, &key)
    }
    fn resolve_at(
        context: &mut impl ParseContext,
        key: &crate::names::FullName,
    ) -> Result<Option<Self>, AnalysisError> {
        let canonical;
        let key = if context.dialect() == crate::ast::Dialect::sqlite() {
            let Some(identity) = context.sqlite_relation_identity(key) else {
                return Ok(None);
            };
            canonical = identity;
            &canonical
        } else {
            key
        };
        if let Some(table) = context.get_table(&TableName::from(key.clone())) {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            return Ok(Some(Self {
                name: TableName::from(key.clone()),
                columns,
                view_support: None,
                merge_view_support: None,
            }));
        }
        if context
            .get_view(&crate::names::ViewName::from_full_name(key))
            .is_some()
        {
            return Self::from_view(context, key);
        }
        Ok(None)
    }
    fn from_view(
        context: &mut impl ParseContext,
        key: &crate::names::FullName,
    ) -> Result<Option<Self>, AnalysisError> {
        let view = context
            .get_view(&crate::names::ViewName::from_full_name(key))
            .expect("resolved view");
        let (columns, support) = if view.is_materialized() {
            (Vec::new(), ViewUpdateSupport::read_only())
        } else {
            (view.result_columns().column_definitions()?, {
                let Some(support) = ViewUpdateSupport::analyze(context, key)? else {
                    return Ok(None);
                };
                support
            })
        };
        let columns = columns
            .into_iter()
            .map(|column| Self::writable_column(column, &support))
            .collect();
        Ok(Some(Self {
            name: TableName::from(key.clone()),
            columns,
            view_support: Some(support),
            merge_view_support: None,
        }))
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
            .map(|column| Self::merge_column(column, support))
            .collect();
        Ok(Self {
            name: name.clone(),
            columns,
            view_support: None,
            merge_view_support: Some(support.clone()),
        })
    }
    fn writable_column(column: ColumnDefinition, support: &ViewUpdateSupport) -> ColumnDefinition {
        if let Some(name) = column.name()
            && let Some(definition) = support.column_definition(name)
        {
            definition.clone()
        } else {
            column
        }
    }
    fn merge_column(column: ColumnDefinition, support: &ViewMergeSupport) -> ColumnDefinition {
        if let Some(name) = column.name()
            && let Some(definition) = support.update_column_definition(name)
        {
            definition.clone()
        } else {
            column
        }
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
