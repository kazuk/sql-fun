use crate::context::schema_file_context::{SchemaCatalog, SchemaFileContext};
use crate::{
    TrieMap,
    context::{ParseContext, ViewInfoRead, ViewInfoWrite},
    sem::{
        AlterView, AnalysisError, AnalysisProblem, CreateTableAs, CreateView, FullName, ViewLike,
    },
    syn::ObjectType,
};

#[derive(Clone, Debug, Default)]
pub struct ViewInformationCollection(TrieMap<ViewLike>, Vec<FullName>);

impl ViewInformationCollection {
    pub(super) fn append_names(&self, names: &mut Vec<FullName>) {
        for name in &self.1 {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    pub(super) fn sqlite_name(&self, name: &FullName) -> Option<FullName> {
        self.1
            .iter()
            .find(|candidate| candidate.sqlite_eq(name))
            .cloned()
    }

    fn get(&self, key: &FullName) -> Option<&ViewLike> {
        self.0.get(&key.to_string())
    }

    pub(super) fn insert(&mut self, key: &FullName, value: &ViewLike) {
        self.0.insert(&key.to_string(), value.clone());
        if !self.1.contains(key) {
            self.1.push(key.clone());
        }
    }
}

impl ViewInfoRead for SchemaCatalog {
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        let identity = self.relation_identity(key);
        let key = &identity;
        if let Some(v) = self.views.get(key) {
            Some(v)
        } else {
            self.base.get_view_impl(key)
        }
    }
}

impl ViewInfoWrite for SchemaFileContext {
    #[tracing::instrument(skip(self))]
    fn apply_alter_view(mut self, alter_view: &AlterView) -> Result<Self, AnalysisError>
    where
        Self: Sized,
    {
        let view_name = alter_view.view();
        let Some((key, mut view)) = crate::sem::search_path::expand_relation_search_path(
            &self,
            view_name.full_name(),
            |key| {
                self.get_view_impl(key)
                    .cloned()
                    .map(|view| (key.clone(), view))
            },
        ) else {
            self.report_analysis_problem(AnalysisProblem::view_not_found(view_name))?;
            return Ok(self);
        };
        if !view.apply_alter(&mut self, alter_view)? {
            return Ok(self);
        }
        self.catalog.store_view(&key, &view)?;
        Ok(self)
    }

    fn apply_create_table_as(
        mut self,
        create_table_as: &CreateTableAs,
    ) -> Result<Self, AnalysisError> {
        if !create_table_as.set_operations_valid()? {
            return Ok(self);
        }
        let name = self.apply_default_schema(create_table_as.name())?;
        match create_table_as.obj_type() {
            ObjectType::ObjectMatview => {
                if !self.permits_relation_creation(
                    &name,
                    crate::sem::Kind::materialized_view(),
                    create_table_as.if_not_exists(),
                    false,
                )? {
                    return Ok(self);
                }
                self.catalog
                    .store_view(&name, &ViewLike::from(create_table_as))?;
                Ok(self)
            }
            _ => {
                todo!()
            }
        }
    }

    #[tracing::instrument(skip(self))]
    fn apply_create_view(mut self, create_view: &CreateView) -> Result<Self, AnalysisError> {
        if !create_view.query().set_operations_valid()? {
            return Ok(self);
        }
        let name = self.creation_name(create_view.name(), create_view.temporary())?;
        let kind = if create_view.is_materialized() {
            crate::sem::Kind::materialized_view()
        } else {
            crate::sem::Kind::view()
        };
        if !self.permits_relation_creation(
            &name,
            kind,
            create_view.if_not_exists(),
            create_view.replace(),
        )? {
            return Ok(self);
        }
        let mut definition = ViewLike::from(create_view);
        if create_view.replace()
            && let Some(previous) = self.get_view_impl(&name)
        {
            if !definition
                .result_columns()
                .can_replace_view_columns(previous.result_columns())
            {
                self.report_analysis_problem(AnalysisProblem::creation(
                    crate::sem::problem::CreationProblem::named(
                        &name,
                        "view replacement",
                        &self.creation_span(),
                    ),
                ))?;
                return Ok(self);
            }
            if let Some(previous) = previous.ordinary_definition()
                && let ViewLike::View(current) = &mut definition
            {
                current.retain_relation_state(previous);
            }
            if let Some(owner) = previous.owner() {
                definition.change_owner(owner);
            }
        }
        self.catalog.store_view(&name, &definition)?;
        Ok(self)
    }
}

#[cfg(test)]
mod test_set_operation_catalog {
    use crate::{
        ast::Dialect,
        context::ViewInfoRead,
        sem::{FullName, PgBuiltInType},
        test_helpers::{ownership::apply_ownership_sql, set_operations},
    };

    #[test]
    fn rejected_set_queries_preserve_catalog_and_allow_following_statements()
    -> testresult::TestResult {
        let context = set_operations::context(Dialect::postgres());
        let initial = apply_ownership_sql(context, "CREATE VIEW kept AS SELECT 1::bigint AS x")?;
        let rejected = apply_ownership_sql(
            initial.into_context(),
            "CREATE OR REPLACE VIEW kept AS SELECT 1 AS x UNION SELECT 'x'::text",
        )?;
        let name = FullName::with_schema("public", "kept");
        let kept = rejected.context().get_view_impl(&name).unwrap();
        assert_eq!(
            kept.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int8())
        );
        let rejected = apply_ownership_sql(
            rejected.into_context(),
            "CREATE MATERIALIZED VIEW absent AS SELECT 1 UNION SELECT 1,2",
        )?;
        assert!(
            rejected
                .context()
                .get_view_impl(&FullName::with_schema("public", "absent"))
                .is_none()
        );
        let recovered = apply_ownership_sql(
            rejected.into_context(),
            "CREATE VIEW next AS SELECT 1 AS x UNION SELECT 2::bigint",
        )?;
        let next = recovered
            .context()
            .get_view_impl(&FullName::with_schema("public", "next"))
            .unwrap();
        assert_eq!(
            next.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int8())
        );
        assert_eq!(recovered.context().problems().len(), 2);
        Ok(())
    }
}
