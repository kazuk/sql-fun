use crate::{
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterView, AnalysisError, ColumnDefinition, ColumnRename, CreateTableAs, CreateView,
        OwnerSpecification, ResultColumnCollection, RuleIdentity, RuleReferences, SelectStatement,
        TriggerReferences, traversal::SemanticVisitorMut,
    },
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
    pub(crate) fn query(&self) -> &SelectStatement {
        match self {
            Self::View(view) => view.query(),
            Self::MaterializedView(view) => view.query(),
        }
    }

    pub(crate) fn walk_column_dependencies(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::View(view) => view.walk_column_dependencies(visitor),
            Self::MaterializedView(view) => view.walk_column_dependencies(visitor),
        }
    }

    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        match self {
            Self::View(value) => value.rename_column(rename),
            Self::MaterializedView(value) => value.rename_column(rename),
        }
    }

    pub(crate) fn rename_column_with(
        &mut self,
        rename: &ColumnRename,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::View(view) => view.rename_column_with(rename, visitor),
            Self::MaterializedView(view) => view.walk_column_dependencies(visitor),
        }
    }
    pub(crate) fn rename_relation(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::View(value) => value.rename_relation(old, new),
            Self::MaterializedView(value) => value.rename_relation(old, new),
        }
    }

    /// Ordinary View semantic definition; materialized views have a different definition.
    pub fn ordinary_definition(&self) -> Option<&CreateView> {
        if let Self::View(view) = self {
            Some(view)
        } else {
            None
        }
    }

    pub(crate) fn with_rule_query(&self, query: &SelectStatement) -> Option<CreateView> {
        if let Self::View(view) = self {
            let mut view = view.as_ref().clone();
            view.replace_rule_query(query);
            Some(view.with_replace(true))
        } else {
            None
        }
    }

    /// Rule links of an ordinary view; materialized views cannot own user rules.
    pub fn rules(&self) -> Option<&RuleReferences> {
        match self {
            Self::View(view) => Some(view.rules()),
            Self::MaterializedView(_) => None,
        }
    }
    pub(crate) fn associate_rule(&mut self, identity: &RuleIdentity) {
        if let Self::View(view) = self {
            view.associate_rule(identity);
        }
    }
    pub(crate) fn dissociate_rule(&mut self, identity: &RuleIdentity) {
        if let Self::View(view) = self {
            view.dissociate_rule(identity);
        }
    }

    /// Ownership of the retained view or materialized-view definition.
    pub fn owner(&self) -> Option<&OwnerSpecification> {
        match self {
            Self::View(view) => view.owner(),
            Self::MaterializedView(view) => view.owner(),
        }
    }
    /// Replace ownership, preserving all view data.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        match self {
            Self::View(view) => view.change_owner(owner),
            Self::MaterializedView(view) => view.change_owner(owner),
        }
    }

    /// Trigger references for an ordinary view. Materialized views cannot own triggers.
    pub fn triggers(&self) -> Option<&TriggerReferences> {
        match self {
            Self::View(view) => Some(view.triggers()),
            Self::MaterializedView(_) => None,
        }
    }
    pub(crate) fn associate_trigger(&mut self, name: &FullName) {
        if let Self::View(view) = self {
            view.associate_trigger(name);
        }
    }
    pub(crate) fn dissociate_trigger(&mut self, name: &FullName) {
        if let Self::View(view) = self {
            view.dissociate_trigger(name);
        }
    }

    pub(crate) fn result_columns(&self) -> &ResultColumnCollection {
        match self {
            Self::View(view) => view.result_columns(),
            Self::MaterializedView(view) => view.result_columns(),
        }
    }

    /// Apply ALTER VIEW; false means a problem was reported without changing the view.
    ///
    /// # Errors
    ///
    /// - `AnalysisError`
    pub fn apply_alter(
        &mut self,
        context: &mut impl ParseContext,
        alter_view: &AlterView,
    ) -> Result<bool, AnalysisError> {
        match self {
            Self::View(create_view) => create_view.apply_alter(context, alter_view),
            Self::MaterializedView(create_table_as) => {
                create_table_as.apply_alter(alter_view).map(|()| true)
            }
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
