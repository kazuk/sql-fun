use crate::{
    sem::{
        AlterView, AnalysisError, AnalysisProblem, CreateTableAs, CreateView, FullName,
        ParseContext, ViewInfoRead, ViewInfoWrite, ViewLike,
    },
    syn::ObjectType,
};

use crate::TrieMap;

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
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
