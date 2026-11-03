use crate::{
    ast::Dialect,
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AnalysisError, ColumnDefinition, Constraint, CreateTable, ParseContext, ViewInsertSupport,
        ViewLike, ViewMergeSupport, search_path::expand_relation_search_path,
    },
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
    pub(crate) const fn is_base_table(&self) -> bool {
        self.table.is_some()
    }

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
            .get_view(&ViewName::from_full_name(name.full_name()))
            .expect("resolved view");
        Self::from_rule_view(&name, view).map(Some)
    }
    fn from_rule_view(name: &TableName, view: &ViewLike) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: name.clone(),
            columns: view.result_columns().column_definitions()?,
            table: None,
            view_support: None,
            merge_view_support: None,
        })
    }
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        let key = expand_relation_search_path(context, requested.full_name(), |key| {
            if context.dialect() == Dialect::sqlite() {
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
        key: &FullName,
    ) -> Result<Option<Self>, AnalysisError> {
        let canonical;
        let key = if context.dialect() == Dialect::sqlite() {
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
                table: Some(table.clone()),
                view_support: None,
                merge_view_support: None,
            }));
        }
        if context.get_view(&ViewName::from_full_name(key)).is_some() {
            return Self::from_view(context, key);
        }
        Ok(None)
    }
    fn from_view(
        context: &mut impl ParseContext,
        key: &FullName,
    ) -> Result<Option<Self>, AnalysisError> {
        let view = context
            .get_view(&ViewName::from_full_name(key))
            .expect("resolved view");
        let (columns, support) = if view.is_materialized() {
            (Vec::new(), ViewInsertSupport::read_only())
        } else {
            (view.result_columns().column_definitions()?, {
                let Some(support) = ViewInsertSupport::analyze(context, key)? else {
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
            table: None,
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
            table: None,
            view_support: None,
            merge_view_support: Some(support.clone()),
        })
    }
    fn writable_column(column: ColumnDefinition, support: &ViewInsertSupport) -> ColumnDefinition {
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
            && let Some(definition) = support.insert_column_definition(name)
        {
            definition.clone()
        } else {
            column
        }
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
