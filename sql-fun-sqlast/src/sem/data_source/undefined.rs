use crate::sem::{
    AliasSpec, AnalysisError, DataSource, FullName, Nullable, SemScalarExpr, data_source::AliasName,
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedTableFuncDataSource {
    func_name: FullName,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
    output: super::TableFunctionOutput,
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
    pub(super) fn from_name(name: &FullName, args: &[SemScalarExpr], alias: &AliasSpec) -> Self {
        Self {
            func_name: name.clone(),
            args: args.to_vec().into(),
            alias: alias.clone(),
            output: super::TableFunctionOutput::default(),
        }
    }

    pub const fn alias(&self) -> &AliasSpec {
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
            output: super::TableFunctionOutput::default(),
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

impl UndefinedTableFuncDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        let mut args = self.args.to_vec();
        for argument in &mut args {
            argument.walk(visitor)?;
        }
        self.args = args.into();
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for argument in self.args.iter() {
            argument.visit(visitor)?;
        }
        Ok(())
    }
}

impl UndefinedTableFuncDataSource {
    pub(super) fn retain_output(&mut self, output: &super::TableFunctionOutput) {
        self.output = output.clone();
    }
    /// Caller-declared columns retained even when function lookup failed.
    pub const fn output_columns(&self) -> &crate::sem::ColumnCollection {
        self.output.columns()
    }
}
