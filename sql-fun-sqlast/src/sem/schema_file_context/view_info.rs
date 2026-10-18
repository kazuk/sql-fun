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
pub struct ViewInformationCollection(TrieMap<ViewLike>);

impl ViewInformationCollection {
    fn get(&self, key: &FullName) -> Option<&ViewLike> {
        self.0.get(&key.catalog_key())
    }

    pub(super) fn insert(&mut self, key: &FullName, value: &ViewLike) {
        self.0.insert(&key.catalog_key(), value.clone());
    }
}

impl ViewInfoRead for SchemaCatalog {
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
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
        let Some((key, mut view)) =
            crate::sem::search_path::expand_search_path_inst(&self, view_name.full_name(), |key| {
                self.get_view_impl(key)
                    .cloned()
                    .map(|view| (key.clone(), view))
            })
        else {
            self.report_analysis_problem(AnalysisProblem::view_not_found(view_name))?;
            return Ok(self);
        };
        view.apply_alter(alter_view)?;
        self.catalog.views.insert(&key, &view);
        Ok(self)
    }

    fn apply_create_table_as(
        mut self,
        create_table_as: &CreateTableAs,
    ) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_table_as.name())?;
        match create_table_as.obj_type() {
            ObjectType::ObjectMatview => {
                self.catalog
                    .views
                    .insert(&name, &ViewLike::from(create_table_as));
                Ok(self)
            }
            _ => {
                todo!()
            }
        }
    }

    #[tracing::instrument(skip(self))]
    fn apply_create_view(mut self, create_view: &CreateView) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_view.name())?;
        self.catalog
            .views
            .insert(&name, &ViewLike::from(create_view));
        Ok(self)
    }
}
