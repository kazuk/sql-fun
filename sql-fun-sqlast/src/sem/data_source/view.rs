use crate::sem::{AliasSpec, ColumnName, DataSource, Nullable, ViewName, data_source::AliasName};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewDataSource {
    view: ViewName,
    alias: AliasSpec,
}

impl ViewDataSource {
    pub(super) fn from_name(view: &ViewName, alias: &AliasSpec) -> Self {
        Self {
            view: view.clone(),
            alias: alias.clone(),
        }
    }

    pub const fn view_name(&self) -> &ViewName {
        &self.view
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        if let Some(view) = context.get_view(&self.view)
            && view.has_column(column)
        {
            names.push(self.alias.clone());
        }
    }
}

impl ViewDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        visitor.relation(None, self.alias.name().as_str(), &[]);
        Ok(())
    }
}
