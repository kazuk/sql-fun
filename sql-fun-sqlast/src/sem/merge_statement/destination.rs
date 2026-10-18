use crate::sem::{
    AnalysisError, InsertDestination, ParseContext, TableName, UpdateDestination, ViewMergeSupport,
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
        context: &impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        crate::sem::search_path::expand_search_path_inst(context, requested.full_name(), |key| {
            let name = TableName::from(key.clone());
            if context.get_table_impl(key).is_some() {
                return Some(Self::resolve_table(context, &name));
            }
            context.get_view_impl(key).map(|view| {
                if view.is_materialized() {
                    let insert = InsertDestination::resolve(context, &name)?
                        .expect("resolved MERGE target materialized view");
                    let update = UpdateDestination::resolve(context, &name)?
                        .expect("resolved MERGE target materialized view");
                    Ok(Self {
                        name,
                        insert,
                        update,
                        view_support: None,
                        materialized_view: true,
                    })
                } else {
                    super::MergeStatement::require_version(context, 17)?;
                    let support = context.view_merge_support(key);
                    let insert = InsertDestination::for_merge_view(&name, view, &support)?;
                    let update = UpdateDestination::for_merge_view(&name, view, &support)?;
                    Ok(Self {
                        name,
                        insert,
                        update,
                        view_support: Some(support),
                        materialized_view: false,
                    })
                }
            })
        })
        .transpose()
    }

    fn resolve_table(context: &impl ParseContext, name: &TableName) -> Result<Self, AnalysisError> {
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
