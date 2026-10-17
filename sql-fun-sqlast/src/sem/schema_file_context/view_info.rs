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
        self.0.get(&key.to_string())
    }

    pub(super) fn insert(&mut self, key: &FullName, value: &ViewLike) {
        self.0.insert(&key.to_string(), value.clone());
    }
}

impl ViewInfoRead for SchemaCatalog {
    fn view_trigger_target(&self, trigger: &FullName) -> Option<FullName> {
        for (_, info) in self.view_writes.iter() {
            if info.has_trigger(trigger) {
                return Some(info.name().clone());
            }
        }
        let target = self.base.view_trigger_target(trigger)?;
        self.view_write_info(&target)
            .filter(|info| info.has_trigger(trigger))
            .map(|_| target)
    }

    fn view_write_info(&self, view: &FullName) -> Option<&crate::sem::ViewWriteInfo> {
        if let Some(info) = self.view_writes.get(&view.to_string()) {
            Some(info)
        } else {
            self.base.view_write_info(view)
        }
    }
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        if let Some(v) = self.views.get(key) {
            Some(v)
        } else {
            self.base.get_view_impl(key)
        }
    }
}

impl ViewInfoWrite for SchemaFileContext {
    fn apply_view_write_info(
        mut self,
        view: &FullName,
        info: &crate::sem::ViewWriteInfo,
    ) -> Result<Self, AnalysisError> {
        self.catalog
            .view_writes
            .insert(&view.to_string(), info.clone());
        Ok(self)
    }
    fn apply_view_write_change(
        mut self,
        change: &crate::sem::ViewWriteChange,
    ) -> Result<Self, AnalysisError> {
        let Some(mut info) = self.view_write_info(change.view()).cloned() else {
            self.report_analysis_problem(AnalysisProblem::view(
                &crate::sem::problem::ViewProblem::missing_metadata(change.view()),
            ))?;
            return Ok(self);
        };
        if let Err(problem) = info.apply(change) {
            self.report_analysis_problem(AnalysisProblem::view(&problem))?;
            return Ok(self);
        }
        self.catalog
            .view_writes
            .insert(&change.view().to_string(), info);
        Ok(self)
    }

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
