use crate::{
    names::{AliasName, FullName},
    sem::{
        AliasSpec, AnalysisError, ColumnCollection, DataSource, Nullable, SemScalarExpr,
        data_source::TableFunctionOutput,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedTableFuncDataSource {
    func_name: FullName,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
    output: TableFunctionOutput,
}

impl UndefinedTableFuncDataSource {
    pub(super) fn from_name(name: &FullName, args: &[SemScalarExpr], alias: &AliasSpec) -> Self {
        Self {
            func_name: name.clone(),
            args: args.to_vec().into(),
            alias: alias.clone(),
            output: TableFunctionOutput::default(),
        }
    }

    pub const fn alias(&self) -> &AliasSpec {
        &self.alias
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
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        let mut args = self.args.to_vec();
        for argument in &mut args {
            argument.walk(visitor)?;
        }
        self.args = args.into();
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for argument in self.args.iter() {
            argument.visit(visitor)?;
        }
        Ok(())
    }
}

impl UndefinedTableFuncDataSource {
    pub(super) fn retain_output(&mut self, output: &TableFunctionOutput) {
        self.output = output.clone();
    }
    /// Caller-declared columns retained even when function lookup failed.
    pub const fn output_columns(&self) -> &ColumnCollection {
        self.output.columns()
    }
}
