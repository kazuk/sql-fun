use crate::{
    ast::TryFromWithTokens,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnCollection, DataSource, FromClause,
        FullName, Nullable, ParseContext, ScalarAnalysisContext, SemScalarExpr, WithClause,
        data_source::AliasName, type_system::ArgumentBindingCollection,
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// row functions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRowFunctions {
    alias: AliasSpec,
    functions: IVec<DataSourceRowsFromFunc>,
}

impl DataSourceRowFunctions {
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
            alias: alias.clone(),
        }
    }

    /// get alias name
    #[must_use]
    pub fn get_aliases(&self) -> Vec<AliasSpec> {
        vec![self.alias.clone()]
    }
}

/// rows from function
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DataSourceRowsFromFunc {
    /// resolved table function call
    FuncCall(FullName, Vec<SemScalarExpr>, ColumnCollection),
    /// undefined table function call
    UndefinedFuncCall(FullName, Vec<SemScalarExpr>),
}

impl DataSourceRowsFromFunc {
    pub(crate) fn resolve<C: crate::sem::ParseContext>(
        context: &mut C,
        name: &FullName,
        arguments: &[SemScalarExpr],
    ) -> Result<Self, AnalysisError> {
        let types = ArgumentBindingCollection::from_expr_list(arguments);
        let Some(overloads) = context.get_function_by_name(name) else {
            context.report_analysis_problem(AnalysisProblem::table_function_not_found(name))?;
            return Ok(Self::UndefinedFuncCall(name.clone(), arguments.to_vec()));
        };
        let Some(overload) = overloads.resolve_from_catalog(context, &types) else {
            context.report_analysis_problem(
                AnalysisProblem::function_overload_resolution_failed(name, &types),
            )?;
            return Ok(Self::UndefinedFuncCall(name.clone(), arguments.to_vec()));
        };
        Ok(Self::FuncCall(
            context.resolve_function_name_with_search_path(name),
            arguments.to_vec(),
            overload.returning_table(),
        ))
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
        match self {
            Self::FuncCall(_, arguments, _) | Self::UndefinedFuncCall(_, arguments) => {
                for argument in arguments {
                    argument.walk(visitor)?;
                }
            }
        }
        Ok(())
    }
}
