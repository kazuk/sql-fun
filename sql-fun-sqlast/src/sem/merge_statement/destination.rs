use crate::{
    names::TableName,
    sem::{
        AnalysisError, InsertDestination, ParseContext, UpdateDestination, ViewLike,
        ViewMergeSupport,
    },
};

/// MERGE-owned projection of a bound table or view target.
pub(crate) struct MergeDestination {
    name: TableName,
    insert: InsertDestination,
    update: UpdateDestination,
    view_support: Option<ViewMergeSupport>,
    materialized_view: bool,
}
impl MergeDestination {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        let Some(identity) = context.resolve_relation_name(requested.full_name()) else {
            return Ok(None);
        };
        let name = TableName::from(identity);
        if context.get_table(&name).is_some() {
            return Self::resolve_table(context, &name).map(Some);
        }
        let view = context
            .get_view(&crate::names::ViewName::from_full_name(name.full_name()))
            .expect("resolved view");
        if view.is_materialized() {
            Self::resolve_materialized_view(context, &name).map(Some)
        } else {
            Self::resolve_view(context, &name, view).map(Some)
        }
    }

    fn resolve_table(
        context: &mut impl ParseContext,
        name: &TableName,
    ) -> Result<Self, AnalysisError> {
        let insert =
            InsertDestination::resolve(context, name)?.expect("resolved MERGE target table");
        let update =
            UpdateDestination::resolve(context, name)?.expect("resolved MERGE target table");
        Ok(Self {
            name: name.clone(),
            insert,
            update,
            view_support: None,
            materialized_view: false,
        })
    }

    fn resolve_materialized_view(
        context: &mut impl ParseContext,
        name: &TableName,
    ) -> Result<Self, AnalysisError> {
        let insert = InsertDestination::resolve(context, name)?
            .expect("resolved MERGE target materialized view");
        let update = UpdateDestination::resolve(context, name)?
            .expect("resolved MERGE target materialized view");
        Ok(Self {
            name: name.clone(),
            insert,
            update,
            view_support: None,
            materialized_view: true,
        })
    }

    fn resolve_view(
        context: &impl ParseContext,
        name: &TableName,
        view: &ViewLike,
    ) -> Result<Self, AnalysisError> {
        super::MergeStatement::require_version(context, 17)?;
        let support = context.view_merge_support(name.full_name());
        let insert = InsertDestination::for_merge_view(name, view, &support)?;
        let update = UpdateDestination::for_merge_view(name, view, &support)?;
        Ok(Self {
            name: name.clone(),
            insert,
            update,
            view_support: Some(support),
            materialized_view: false,
        })
    }

    pub(crate) const fn name(&self) -> &TableName {
        &self.name
    }

    pub(crate) const fn insert(&self) -> &InsertDestination {
        &self.insert
    }

    pub(crate) const fn update(&self) -> &UpdateDestination {
        &self.update
    }

    pub(crate) const fn view_support(&self) -> Option<&ViewMergeSupport> {
        self.view_support.as_ref()
    }

    pub(crate) const fn is_materialized_view(&self) -> bool {
        self.materialized_view
    }
}
