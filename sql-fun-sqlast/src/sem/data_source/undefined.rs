use sql_fun_core::IVec;

use crate::sem::{
    AliasSpec, AnalysisError, DataSource, FullName, Nullable, SemScalarExpr, data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedTableFuncDataSource {
    func_name: FullName,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
}

impl super::DataSource {
    /// create datasource for undefined table function
    pub(crate) fn undefined_table_function(
        call: &crate::syn::FuncCall,
        arg_exprs: IVec<SemScalarExpr>,
        alias_name: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        UndefinedTableFuncDataSource::new_data_source(call, arg_exprs, alias_name)
    }
}

impl UndefinedTableFuncDataSource {
    pub fn alias(&self) -> &AliasSpec {
        &self.alias
    }

    pub(super) fn new_data_source(
        func_call: &crate::syn::FuncCall,
        args: IVec<SemScalarExpr>,
        alias: &AliasSpec,
    ) -> Result<DataSource, AnalysisError> {
        let func_name = FullName::try_from(func_call.get_funcname())?;

        Ok(DataSource::UndefinedTableFunc(Self {
            func_name,
            args,
            alias: alias.clone(),
        }))
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
}
