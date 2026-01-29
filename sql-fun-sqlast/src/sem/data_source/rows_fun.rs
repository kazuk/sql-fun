use sql_fun_core::IVec;

use crate::{
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnCollection, DataSource, FromClause,
        FullName, Nullable, ParseContext, SemScalarExpr, WithClause, analyze_scaler_expr,
        data_source::AliasName, type_system::ArgumentBindingCollection,
    },
    syn::{ListOpt, Opt, ScanToken},
};

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
        let fun_name = FullName::try_from(func_call.get_funcname())?;
        let Some(args) = func_call.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("func_call.args")?
        };
        let mut arg_exprs = Vec::new();
        let mut arg_types = ArgumentBindingCollection::default();
        for arg in args {
            let (arg, new_context) =
                analyze_scaler_expr(context, with_clause, from_clause, arg, tokens)?;
            context = new_context;
            arg_types.push(arg.get_type());
            arg_exprs.push(arg);
        }

        if let Some(overloads) = context.get_function_by_name(&fun_name) {
            let Some(overload) = overloads.resolve_overload(&mut context, &arg_types) else {
                context.report_problem(AnalysisProblem::function_overload_resolution_failed(
                    &fun_name, &arg_types,
                ))?;
                return Ok((Self::UndefinedFuncCall(fun_name, arg_exprs), context));
            };
            let columns = overload.returning_table();
            let resolved_fun_name = context.resolve_function_name_with_search_path(&fun_name);
            Ok((
                Self::FuncCall(resolved_fun_name, arg_exprs, columns),
                context,
            ))
        } else {
            context.report_problem(AnalysisProblem::table_function_not_found(&fun_name))?;
            Ok((Self::UndefinedFuncCall(fun_name, arg_exprs), context))
        }
    }

    /// analyze Node in RangeFunction.functions
    pub fn analyze_node<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        function: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(func_call) = function.as_func_call().as_inner() {
            Self::analyze_func_call(context, with_clause, from_clause, func_call, tokens)
        } else if let Some(_sqlvf) = function.as_sqlvalue_function().as_inner() {
            todo!()
        } else {
            AnalysisError::raise_unexpected_input(
                "RangeFunction.functions contains unexpected node",
            )
        }
    }
}
