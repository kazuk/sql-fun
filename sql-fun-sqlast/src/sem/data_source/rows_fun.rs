use crate::{
    ast::TryFromWithTokens,
    sem::{
        AliasSpec, AnalysisError, DataSource, FromClause, FullName, Nullable, ParseContext,
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
    ) -> Result<Vec<super::ProjectionColumn<'a>>, AnalysisError> {
        let mut columns = Vec::new();
        for function in &self.functions {
            function.append_projection(context, with, &mut columns)?;
        }
        if self.ordinality {
            let name = crate::sem::ColumnName::from("ordinality");
            let definition = crate::sem::ColumnDefinition::new(
                &Some(name.clone()),
                Some(&crate::sem::PgBuiltInType::int8()),
                Some(true),
                &[],
            );
            let expression =
                crate::sem::ColumnReferenceExpr::function_output(&self.alias, &definition);
            columns.push(super::ProjectionColumn::expression(&name, &expression));
        }
        Ok(columns
            .into_iter()
            .enumerate()
            .map(|(index, column)| {
                let column = if let Some(name) = self.alias.derived_column_at(index) {
                    column.with_name(name)
                } else {
                    column
                };
                column.with_join_alias(self.alias.name())
            })
            .collect())
    }
    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<crate::sem::ColumnName>, AnalysisError> {
        Ok(self
            .project(context, with)?
            .into_iter()
            .map(|column| column.name().clone())
            .collect())
    }
    fn known_column_names(&self) -> Option<Vec<crate::sem::ColumnName>> {
        let mut names = Vec::new();
        for function in &self.functions {
            function.append_known_column_names(&mut names)?;
        }
        if self.ordinality {
            names.push(crate::sem::ColumnName::from("ordinality"));
        }
        Some(
            names
                .into_iter()
                .enumerate()
                .map(|(index, name)| self.alias.derived_column_at(index).cloned().unwrap_or(name))
                .collect(),
        )
    }
    pub(super) fn candidates(&self, name: &crate::sem::ColumnName) -> super::ColumnCandidates {
        let Some(names) = self.known_column_names() else {
            return super::ColumnCandidates::unknown();
        };
        let mut candidates = Vec::new();
        for column in names {
            if &column == name {
                candidates.push(self.alias.clone());
            }
        }
        super::ColumnCandidates::from(candidates)
    }
    pub(super) fn bind_column<'a, C: ParseContext>(
        &self,
        context: &'a mut C,
        with: &WithClause,
        alias: &AliasName,
        name: &crate::sem::ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), AnalysisError> {
        let mut matches = self
            .project(context, with)?
            .into_iter()
            .filter(|column| column.name() == name);
        let Some(column) = matches.next() else {
            return crate::sem::ColumnReferenceExpr::function_source_ref(
                context,
                alias,
                name,
                None,
                None,
                &IVec::default(),
                nullable,
            );
        };
        if matches.next().is_some() {
            return Ok((
                crate::sem::ColumnReferenceExpr::ambiguous_column_resolved(context, name)?,
                context,
            ));
        }
        Ok((
            column
                .with_nullability(nullable)
                .bind_direct(context, with)?,
            context,
        ))
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
        columns: &mut Vec<super::ProjectionColumn<'a>>,
    ) -> Result<(), AnalysisError> {
        columns.extend(self.source.projection_names(context, with, None)?);
        Ok(())
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
    fn append_known_column_names(&self, names: &mut Vec<crate::sem::ColumnName>) -> Option<()> {
        names.extend(self.source.known_function_columns()?);
        Some(())
    }
}
