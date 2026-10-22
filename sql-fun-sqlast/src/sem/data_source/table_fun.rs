use crate::sem::{
    AliasSpec, ColumnName, DataSource, Nullable, OverloadVariant, SemScalarExpr, WithClause,
    data_source::AliasName,
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionDataSource {
    function: OverloadVariant,
    function_name: Option<crate::sem::QueryName>,
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
    /// Requested function name and its recorded search-path resolution.
    pub const fn function_name(&self) -> Option<&crate::sem::QueryName> {
        self.function_name.as_ref()
    }
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
            function_name: None,
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
        let (expression, context) = crate::sem::ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            Some(&self.function),
            self.output.column(column, &self.alias),
            &self.args,
            nullable,
        )?;
        Ok((
            expression.with_function_type_declaration(self.output.declaration(column, &self.alias)),
            context,
        ))
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
    /// Caller-supplied type and modifiers for an effective output column.
    pub fn output_type_declaration(
        &self,
        column: &ColumnName,
    ) -> Option<&crate::sem::WithTypmod<crate::sem::TypeReference>> {
        self.output.declaration(column, &self.alias)
    }
    /// Effective invocation columns, independently of the catalog return declaration.
    pub const fn output_columns(&self) -> &crate::sem::ColumnCollection {
        self.output.columns()
    }
    /// Retained invalid-definition outcomes, independently of diagnostic delivery.
    pub fn output_problems(&self) -> &[crate::sem::TableFunctionProblem] {
        self.output.problems()
    }
}

impl DataSource {
    pub(crate) fn record_function_name(
        &mut self,
        requested: &crate::sem::FullName,
        resolved: &crate::sem::FullName,
    ) {
        if let Self::CallTableFunc(source) = self {
            source.function_name = Some(crate::sem::QueryName::new(
                &requested.to_string(),
                crate::sem::QueryNameResolution::function(resolved),
            ));
        }
    }
}
