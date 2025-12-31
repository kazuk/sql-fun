use crate::{
    TrieMap,
    sem::{
        AlterView, AnalysisError, AnalysisProblem, BaseParseContext, CreateTableAs, CreateView,
        FullName, ViewInfoRead, ViewInfoWrite, ViewLike,
    },
    syn::ObjectType,
};

use super::SchemaFileContext;

#[derive(Debug, Default)]
pub struct ViewInformationCollection(TrieMap<ViewLike>);

impl ViewInformationCollection {
    fn get(&self, key: &FullName) -> Option<&ViewLike> {
        self.0.get(&key.to_string())
    }

    fn insert(&mut self, key: &FullName, value: &ViewLike) {
        self.0.insert(&key.to_string(), value.clone());
    }
}

impl<TBaseContext> ViewInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        if let Some(v) = self.views.get(key) {
            Some(v)
        } else {
            self.base.get_view_impl(key)
        }
    }
}

impl<TBaseContext> ViewInfoWrite for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    #[tracing::instrument(skip(self))]
    fn apply_alter_view(mut self, alter_view: &AlterView) -> Result<Self, AnalysisError>
    where
        Self: Sized,
    {
        let view_name = alter_view.view();
        let Some(mut view) = self.get_view(view_name).cloned() else {
            self.report_problem(AnalysisProblem::view_not_found(view_name));
            return Ok(self);
        };

        view.apply_alter(alter_view)?;
        self.views.insert(view_name.full_name(), &view);
        Ok(self)
    }

    fn apply_create_table_as(
        mut self,
        create_table_as: &CreateTableAs,
    ) -> Result<Self, AnalysisError> {
        let name = create_table_as.name();
        match create_table_as.obj_type() {
            ObjectType::ObjectMatview => {
                self.views.insert(name, &ViewLike::from(create_table_as));
                Ok(self)
            }
            _ => {
                todo!()
            }
        }
    }

    #[tracing::instrument(skip(self))]
    fn apply_create_view(mut self, create_view: &CreateView) -> Result<Self, AnalysisError> {
        let name = create_view.name();
        self.views.insert(name, &ViewLike::from(create_view));
        Ok(self)
    }
}
