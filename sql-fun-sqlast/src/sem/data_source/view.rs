use crate::sem::{
    AliasSpec, ColumnName, DataSource, Nullable, ParseContext, ViewName, data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewDataSource {
    view: ViewName,
    alias: AliasSpec,
}

impl ViewDataSource {
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
        TParseContext: ParseContext,
    {
        if let Some(view) = context.get_view(&self.view)
            && view.has_column(column)
        {
            names.push(self.alias.clone());
        }
    }

    pub(super) fn new_data_source(
        range_var: &crate::syn::RangeVar,
        alias_name: &AliasSpec,
    ) -> DataSource {
        DataSource::View(Self {
            view: ViewName::from(range_var),
            alias: alias_name.clone(),
        })
    }
}
