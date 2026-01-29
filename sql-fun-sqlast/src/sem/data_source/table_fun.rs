use sql_fun_core::IVec;

use crate::sem::{
    AliasSpec, ColumnName, DataSource, Nullable, OverloadVariant, ParseContext, SemScalarExpr,
    WithClause, data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionDataSource {
    function: OverloadVariant,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
}

impl super::DataSource {
    /// create datasource for table function
    pub(crate) fn table_function(
        overload: &OverloadVariant,
        arg_exprs: IVec<SemScalarExpr>,
        alias_name: &AliasSpec,
    ) -> Self {
        TableFunctionDataSource::new_data_source(overload.clone(), arg_exprs, alias_name)
    }
}

impl TableFunctionDataSource {
    pub fn function(&self) -> &OverloadVariant {
        &self.function
    }

    pub fn arguments(&self) -> &IVec<SemScalarExpr> {
        &self.args
    }

    pub(super) fn new_data_source(
        function: OverloadVariant,
        args: IVec<SemScalarExpr>,
        alias: &AliasSpec,
    ) -> DataSource {
        DataSource::CallTableFunc(Self {
            function,
            args,
            alias: alias.clone(),
        })
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
        _context: &TParseContext,
        _with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        let returning_table = &self.function.returning_table();
        if returning_table.has_column(column) {
            names.push(self.alias.clone());
        }
    }
}
