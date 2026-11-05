use crate::{
    names::{TableName, ViewName},
    sem::{
        AnalysisError, InsertDestination, ParseContext, UpdateDestination, ViewLike,
        ViewMergeSupport, merge_statement::MergeStatement,
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
            .get_view(&ViewName::from_full_name(name.full_name()))
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
        MergeStatement::require_version(context, 17)?;
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
}

#[cfg(test)]
mod test_automatic_view_requires_postgresql_17_and_preserves_returning_support {
    use crate::test_helpers::{
        document_merge::catalog,
        merge_views::{automatic_support, install_view, view_catalog},
        statements::resolve_postgres as resolve,
    };

    #[test]
    fn automatic_view_requires_postgresql_17_and_preserves_returning_support() {
        let support = automatic_support();
        let mut context = view_catalog(17, &support);
        let statement = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id",
            &mut context,
        )
        .unwrap();
        let merge = statement.as_merge().unwrap();

        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(merge.result_columns().len(), 1);
        assert_eq!(merge.view_support(), Some(&support));

        let mut context = catalog(16);
        install_view(&mut context, false);
        let result = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
            &mut context,
        );
        assert!(result.is_err() || context.reported_problem_count() > 0);
    }
}

impl MergeDestination {
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
