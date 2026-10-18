use crate::sem::{
    AlterView, AnalysisError, ColumnDefinition, ColumnName, CreateTableAs, CreateView,
};

/// view like object
#[derive(Debug, Clone)]
pub enum ViewLike {
    /// normal view
    View(Box<CreateView>),
    /// materialized view
    MaterializedView(CreateTableAs),
}

impl ViewLike {
    /// Ordinary View semantic definition; materialized views have a different definition.
    pub fn ordinary_definition(&self) -> Option<&crate::sem::CreateView> {
        if let Self::View(view) = self {
            Some(view)
        } else {
            None
        }
    }

    pub(crate) fn with_rule_query(
        &self,
        query: &crate::sem::SelectStatement,
    ) -> Option<CreateView> {
        if let Self::View(view) = self {
            let mut view = view.as_ref().clone();
            view.replace_rule_query(query);
            Some(view.with_replace(true))
        } else {
            None
        }
    }

    /// Rule links of an ordinary view; materialized views cannot own user rules.
    pub fn rules(&self) -> Option<&crate::sem::RuleReferences> {
        match self {
            Self::View(view) => Some(view.rules()),
            Self::MaterializedView(_) => None,
        }
    }
    pub(crate) fn associate_rule(&mut self, identity: &crate::sem::RuleIdentity) {
        if let Self::View(view) = self {
            view.associate_rule(identity);
        }
    }
    pub(crate) fn dissociate_rule(&mut self, identity: &crate::sem::RuleIdentity) {
        if let Self::View(view) = self {
            view.dissociate_rule(identity);
        }
    }

    /// Ownership of the retained view or materialized-view definition.
    pub fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        match self {
            Self::View(view) => view.owner(),
            Self::MaterializedView(view) => view.owner(),
        }
    }
    /// Replace ownership, preserving all view data.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        match self {
            Self::View(view) => view.change_owner(owner),
            Self::MaterializedView(view) => view.change_owner(owner),
        }
    }

    /// Trigger references for an ordinary view. Materialized views cannot own triggers.
    pub fn triggers(&self) -> Option<&crate::sem::TriggerReferences> {
        match self {
            Self::View(view) => Some(view.triggers()),
            Self::MaterializedView(_) => None,
        }
    }
    pub(crate) fn associate_trigger(&mut self, name: &crate::sem::FullName) {
        if let Self::View(view) = self {
            view.associate_trigger(name);
        }
    }
    pub(crate) fn dissociate_trigger(&mut self, name: &crate::sem::FullName) {
        if let Self::View(view) = self {
            view.dissociate_trigger(name);
        }
    }

    pub(crate) fn result_columns(&self) -> &crate::sem::ResultColumnCollection {
        match self {
            Self::View(view) => view.result_columns(),
            Self::MaterializedView(view) => view.result_columns(),
        }
    }

    /// apply alter view statement
    ///
    /// # Errors
    ///
    /// - `AnalysisError`
    pub fn apply_alter(&mut self, alter_view: &AlterView) -> Result<(), AnalysisError> {
        match self {
            Self::View(create_view) => create_view.apply_alter(alter_view),
            Self::MaterializedView(create_table_as) => create_table_as.apply_alter(alter_view),
        }
    }

    /// get column definition by column name
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<ColumnDefinition> {
        match self {
            Self::View(create_view) => create_view.get_column_def(column_name),
            Self::MaterializedView(create_table_as) => create_table_as.get_column_def(column_name),
        }
    }

    /// return true if column exist
    #[must_use]
    pub fn has_column(&self, column_name: &ColumnName) -> bool {
        match self {
            Self::View(create_view) => create_view.has_column(column_name),
            Self::MaterializedView(cta) => cta.has_column(column_name),
        }
    }

    /// check view is materialized
    #[must_use]
    pub const fn is_materialized(&self) -> bool {
        matches!(self, Self::MaterializedView(_))
    }
}

impl From<&CreateView> for ViewLike {
    fn from(value: &CreateView) -> Self {
        Self::View(Box::new(value.clone()))
    }
}

impl From<&CreateTableAs> for ViewLike {
    fn from(value: &CreateTableAs) -> Self {
        Self::MaterializedView(value.clone())
    }
}
