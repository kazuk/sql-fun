use crate::sem::{
    AliasSpec, ColumnName, DataSource, Nullable, OverloadVariant, SemScalarExpr, WithClause,
    data_source::AliasName,
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionDataSource {
    function: OverloadVariant,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
    output: super::TableFunctionOutput,
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
    pub const fn function(&self) -> &OverloadVariant {
        &self.function
    }

    pub const fn arguments(&self) -> &IVec<SemScalarExpr> {
        &self.args
    }

    pub(super) fn new_data_source(
        function: OverloadVariant,
        args: IVec<SemScalarExpr>,
        alias: &AliasSpec,
    ) -> DataSource {
        DataSource::CallTableFunc(Self {
            output: super::TableFunctionOutput::declared(&function),
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
        TParseContext: crate::sem::ParseContext,
    {
        names.extend(self.output.candidates(column, &self.alias));
    }
}

impl TableFunctionDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
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
}

impl TableFunctionDataSource {
    pub(super) fn apply_output(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
        output: &super::TableFunctionOutput,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.output = output.clone();
        self.output.apply(context, &self.function, &self.alias)
    }
    pub(super) fn column_names(&self) -> Vec<ColumnName> {
        self.output.names()
    }
    pub(super) fn bind_column<'a, C: crate::sem::ParseContext>(
        &self,
        context: &'a mut C,
        alias: &AliasName,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), crate::sem::AnalysisError> {
        if self.output.candidates(column, &self.alias).len() > 1 {
            return Ok((
                crate::sem::ColumnReferenceExpr::ambiguous_column_resolved(context, column)?,
                context,
            ));
        }
        crate::sem::ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            Some(&self.function),
            self.output.column(column, &self.alias),
            &self.args,
            nullable,
        )
    }
}

impl TableFunctionDataSource {
    pub(super) fn append_ordinality(&mut self) {
        self.output.append_ordinality();
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.output.validate_aliases(context, &self.alias)
    }
}

impl TableFunctionDataSource {
    /// Effective invocation columns, independently of the catalog return declaration.
    pub const fn output_columns(&self) -> &crate::sem::ColumnCollection {
        self.output.columns()
    }
    /// Retained invalid-definition outcomes, independently of diagnostic delivery.
    pub fn output_problems(&self) -> &[crate::sem::TableFunctionProblem] {
        self.output.problems()
    }
}
