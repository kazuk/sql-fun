use crate::sem::{ParseContext, TableName, ViewDeleteSupport};

/// DELETE-owned projection of the bound catalog destination.
pub(crate) struct DeleteDestination {
    name: TableName,
    found: bool,
    view_support: Option<ViewDeleteSupport>,
}
impl DeleteDestination {
    pub(crate) fn resolve(context: &impl ParseContext, requested: &TableName) -> Option<Self> {
        crate::sem::search_path::expand_search_path_inst(context, requested.full_name(), |key| {
            if context.get_table_impl(key).is_some() {
                return Some(Self {
                    name: TableName::from(key.clone()),
                    found: true,
                    view_support: None,
                });
            }
            context.get_view_impl(key).map(|view| {
                let support = if view.is_materialized() {
                    ViewDeleteSupport::read_only()
                } else {
                    context.view_delete_support(key)
                };
                Self {
                    name: TableName::from(key.clone()),
                    found: true,
                    view_support: Some(support),
                }
            })
        })
    }
    pub(crate) fn missing(name: &TableName) -> Self {
        Self {
            name: name.clone(),
            found: false,
            view_support: None,
        }
    }
    pub(crate) const fn found(&self) -> bool {
        self.found
    }
    pub(crate) const fn name(&self) -> &TableName {
        &self.name
    }
    pub(crate) const fn view_support(&self) -> Option<&ViewDeleteSupport> {
        self.view_support.as_ref()
    }
    pub(crate) fn is_deletable(&self) -> bool {
        self.view_support
            .as_ref()
            .is_none_or(ViewDeleteSupport::is_deletable)
    }
}
