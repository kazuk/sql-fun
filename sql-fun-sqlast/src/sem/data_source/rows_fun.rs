use crate::{
    ast::TryFromWithTokens,
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, DataSource, FromClause, FullName, Nullable,
        ScalarAnalysisContext, SemScalarExpr, WithClause, data_source::AliasName,
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// row functions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRowFunctions {
    alias: AliasSpec,
    functions: IVec<DataSourceRowsFromFunc>,
    ordinality: bool,
    outer_definition: super::TableFunctionOutput,
}

impl DataSourceRowFunctions {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.functions
            .iter()
            .all(|function| function.source().projection_shape_is_resolved())
    }
    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.functions
            .iter()
            .any(|function| function.source().projection_is_table_dependent(context))
    }

    pub(super) fn matches_alias(&self, name: &AliasName) -> bool {
        self.alias.name() == name
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

    pub(super) fn new(functions: &IVec<DataSourceRowsFromFunc>, alias: &AliasSpec) -> Self {
        Self {
            functions: functions.clone(),
            ordinality: false,
            outer_definition: super::TableFunctionOutput::default(),
            alias: alias.clone(),
        }
    }

    /// Calls supplied to ROWS FROM, in written order.
    pub const fn functions(&self) -> &IVec<DataSourceRowsFromFunc> {
        &self.functions
    }

    /// Names retained on resolved invocations, in call order.
    /// Deferred or unresolved calls without a resolved invocation are omitted.
    pub fn resolved_function_names(&self) -> Vec<crate::sem::QueryName> {
        self.functions
            .iter()
            .filter_map(|function| function.source().resolved_function_name().cloned())
            .collect()
    }

    /// get alias name
    #[must_use]
    pub fn get_aliases(&self) -> Vec<AliasSpec> {
        vec![self.alias.clone()]
    }
}

/// One ROWS FROM invocation, retaining its signature and output shape.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRowsFromFunc {
    source: Box<DataSource>,
}
impl DataSourceRowsFromFunc {
    fn wildcard_layout(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<crate::sem::WildcardRowsFunction, AnalysisError> {
        let names = self.source.column_names(context, with)?;
        Ok(crate::sem::WildcardRowsFunction::from_columns(&names))
    }

    pub(crate) fn invocation(source: &DataSource) -> Self {
        Self {
            source: Box::new(source.clone()),
        }
    }
    pub(crate) fn default_alias(&self) -> String {
        self.source
            .get_aliases()
            .first()
            .map(|alias| alias.name().as_str().to_owned())
            .unwrap_or_default()
    }
    pub(crate) fn deferred(call: &crate::sem::FuncCallExpr) -> Self {
        Self::invocation(&DataSource::deferred_table_function(
            call,
            &AliasSpec::from_local_name(&call.name().local_name()),
        ))
    }
    /// Preserved invocation when its signature is deferred.
    pub fn deferred_function(&self) -> Option<&crate::sem::FuncCallExpr> {
        self.source.deferred_function().map(|source| source.call())
    }
    /// Function source, including the selected overload and invocation output.
    pub fn source(&self) -> &DataSource {
        &self.source
    }
    pub(crate) fn resolve<C: ParseContext>(
        context: &mut C,
        name: &FullName,
        arguments: &[SemScalarExpr],
    ) -> Result<Self, AnalysisError> {
        let mut source = DataSource::resolve_function(
            context,
            name,
            arguments,
            &AliasSpec::from_local_name(&name.local_name()),
        )?;
        source.apply_function_output(context, &super::TableFunctionOutput::default())?;
        Ok(Self::invocation(&source))
    }
    /// analyze `FuncCall` node in RangeFunction.functions
    pub fn analyze_func_call<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        func_call: crate::syn::FuncCall,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Expr::try_from_with_tokens(&func_call, tokens)?;
        let result = syntax.resolve_rows(
            &mut context,
            &ScalarAnalysisContext::new(with_clause, from_clause),
        );
        Ok((result?, context))
    }

    /// analyze Node in RangeFunction.functions
    pub fn analyze_node<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        function: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Expr::try_from_with_tokens(&function, tokens)?;
        let result = syntax.resolve_rows(
            &mut context,
            &ScalarAnalysisContext::new(with_clause, from_clause),
        );
        Ok((result?, context))
    }
}

impl DataSourceRowFunctions {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        let mut functions = self.functions.to_vec();
        for function in &mut functions {
            function.walk(visitor)?;
        }
        self.functions = functions.into();
        Ok(())
    }
}
impl DataSourceRowsFromFunc {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.source.walk(visitor)?;
        Ok(())
    }
}

impl DataSourceRowFunctions {
    pub(super) fn project<'a>(
        &'a self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<super::projection::ProjectionColumns<'a>, AnalysisError> {
        let mut columns = super::projection::ProjectionColumns::default();
        for function in &self.functions {
            function.append_projection(context, with, &mut columns)?;
        }
        if self.ordinality {
            columns.append_ordinality(&self.alias);
        }
        let columns = columns.with_aliases(&self.alias);
        if self.projection_is_table_dependent(context) {
            let functions = self
                .functions
                .iter()
                .map(|function| function.wildcard_layout(context, with))
                .collect::<Result<Vec<_>, _>>()?;
            let source = crate::sem::WildcardRowsSource::new(
                self.alias.name(),
                &functions,
                &columns.names(),
                self.ordinality,
            );
            Ok(columns.with_rows_source(&source))
        } else {
            Ok(columns)
        }
    }
    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<crate::sem::ColumnName>, AnalysisError> {
        Ok(self.project(context, with)?.names())
    }
    fn known_column_names(&self) -> Option<super::function_columns::OutputColumnNames> {
        let mut names = super::function_columns::OutputColumnNames::default();
        for function in &self.functions {
            function.append_known_column_names(&mut names)?;
        }
        if self.ordinality {
            names.append_ordinality();
        }
        Some(names)
    }
    pub(super) fn candidates(&self, name: &crate::sem::ColumnName) -> super::ColumnCandidates {
        let Some(names) = self.known_column_names() else {
            return super::ColumnCandidates::unknown();
        };
        super::ColumnCandidates::from(names.candidates(name, &self.alias))
    }
    pub(super) fn bind_column<'a, C: ParseContext>(
        &self,
        context: &'a mut C,
        with: &WithClause,
        alias: &AliasName,
        name: &crate::sem::ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), AnalysisError> {
        let expression = self
            .project(context, with)?
            .bind_column(context, with, alias, name, nullable)?;
        Ok((expression, context))
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let Some(names) = self.known_column_names() else {
            return Ok(());
        };
        self.outer_definition.validate_alias_count(
            context,
            self.alias.len_derived_column(),
            names.len(),
        )
    }
}
impl DataSourceRowsFromFunc {
    fn append_projection<'a>(
        &'a self,
        context: &impl ParseContext,
        with: &WithClause,
        columns: &mut super::projection::ProjectionColumns<'a>,
    ) -> Result<(), AnalysisError> {
        columns.append_source(&self.source, context, with)
    }
}

impl DataSourceRowFunctions {
    pub(super) const fn append_ordinality(&mut self) {
        self.ordinality = true;
    }
}

impl DataSourceRowFunctions {
    pub(super) fn retain_outer_definition(&mut self, definition: &super::TableFunctionOutput) {
        self.outer_definition = definition.clone();
    }
}

impl DataSourceRowsFromFunc {
    fn append_known_column_names(
        &self,
        names: &mut super::function_columns::OutputColumnNames,
    ) -> Option<()> {
        names.append_function_columns(&self.source)
    }
}

impl DataSourceRowFunctions {
    pub(super) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<super::ProjectionColumn<'_>>, AnalysisError> {
        if qualifier.is_some_and(|name| !self.matches_alias(name)) {
            return Ok(Vec::new());
        }
        Ok(self.project(context, with)?.into_columns())
    }
}
