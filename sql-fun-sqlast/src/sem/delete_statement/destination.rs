use crate::sem::search_path::expand_relation_search_path;
use crate::sem::{ParseContext, TableName, ViewDeleteSupport};

/// DELETE-owned projection of the bound catalog destination.
pub(crate) struct DeleteDestination {
    name: TableName,
    found: bool,
    view_support: Option<ViewDeleteSupport>,
}
impl DeleteDestination {
    /// Bind a rule action destination without evaluating later execution eligibility.
    pub(crate) fn resolve_rule_target(
        context: &impl ParseContext,
        requested: &TableName,
    ) -> Option<Self> {
        context
            .resolve_relation_name(requested.full_name())
            .map(|name| Self {
                name: TableName::from(name),
                found: true,
                view_support: None,
            })
    }

    pub(crate) fn resolve(context: &impl ParseContext, requested: &TableName) -> Option<Self> {
        expand_relation_search_path(context, requested.full_name(), |key| {
            if context.get_table_impl(key).is_some() {
                return Some(Self {
                    name: TableName::from(key.clone()),
                    found: true,
                    view_support: None,
                });
            }
            context
                .get_view(&crate::sem::ViewName::from_full_name(key))
                .map(|view| {
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
