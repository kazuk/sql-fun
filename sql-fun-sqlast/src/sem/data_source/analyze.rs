mod impl_analyze_range_var_node;

use crate::{
    AstAndContextPair,
    sem::{
        AliasSpec, AnalysisError, DataSourceRowFunctions, DataSourceRowsFromFunc, FromClause,
        FullName, JoinKind, ParseContext, SemAst, WithClause, analyze_node,
        data_source::JoinExprDataSource,
    },
    syn::{ListOpt, Opt},
};

use super::{DataSource, SubQueryDataSource};

impl DataSource {
    // subquery can not to use CTEs, not need to with_clause
    fn analyze_range_subselect<TParseContext>(
        mut context: TParseContext,
        range_subsel: crate::syn::RangeSubselect,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(alias) = range_subsel.get_alias().as_inner() else {
            AnalysisError::raise_unexpected_none("range_subsel.alias")?
        };
        let (alias_name, new_context) = AliasSpec::analyze_without_relname(context, &alias)?;
        context = new_context;

        let Some(query) = range_subsel.get_subquery().as_inner() else {
            AnalysisError::raise_unexpected_none("range_subselect.subquery")?
        };
        let AstAndContextPair(sem, new_context) = analyze_node(context, &None, query)?;
        let SemAst::SelectStatement(s) = sem else {
            AnalysisError::raise_unexpected_none("subquery is not select?")?
        };
        Ok((
            SubQueryDataSource::new_data_source(&s, &alias_name),
            new_context,
        ))
    }
}

#[cfg(test)]
mod test_analyze_range_subselect {
    use testresult::TestResult;

    use crate::{
        syn::{Node, NodeList, WithClauseOpt},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_range_subselect(test_context: TestParseContext) -> TestResult {
        let builder = SynBuilder::new();

        let res_target = builder.res_target(
            "name",
            crate::syn::NodeInner::AConst(builder.const_int4(10)).into(),
        );
        let query = builder.select_stmt(
            WithClauseOpt::none(),
            NodeList::from(Vec::<Node>::new()),
            vec![res_target],
        );
        let alias_spec = builder.alias_name_only("alias1");
        let range_subselect = builder.range_subselect(alias_spec, query);

        super::DataSource::analyze_range_subselect(test_context, range_subselect)?;
        Ok(())
    }
}

impl DataSource {
    fn analyze_join_expr<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        join_expr: crate::syn::JoinExpr,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(left) = join_expr.get_larg().as_inner() else {
            AnalysisError::raise_unexpected_none("join_expr.larg")?
        };
        let Some(right) = join_expr.get_rarg().as_inner() else {
            AnalysisError::raise_unexpected_none("join_expr.rarg")?
        };

        let (left_source, new_context) =
            Self::analyze_from_clause_node(context, with_clause, from_clause, left)?;
        let (right_source, new_context) =
            Self::analyze_from_clause_node(new_context, with_clause, from_clause, right)?;

        let join_kind = JoinKind::try_from(join_expr.get_jointype())?;
        Ok((
            JoinExprDataSource::new_data_source(&join_kind, &left_source, &right_source),
            new_context,
        ))
    }

    fn analyze_range_table_func<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeTableFunc,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(row_expr) = node.get_rowexpr().as_inner() else {
            AnalysisError::raise_unexpected_none("RangeTableFunc.rowexpr")?
        };
        let Some(call) = row_expr.as_func_call().as_inner() else {
            AnalysisError::raise_unexpected_none("RangeTableFunc.rowexpr is not FuncCall")?
        };
        let _func_name = FullName::try_from(call.get_funcname())?;

        Self::call_table_func(
            context,
            with_clause,
            from_clause,
            call,
            node.get_alias().as_inner(),
        )
    }

    fn analyze_range_func_rows_from<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeFunction,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(alias) = node.get_alias().as_inner() else {
            AnalysisError::raise_unexpected_none("RangeFunction.alias")?
        };
        let (alias, new_context) = AliasSpec::analyze_without_relname(context, &alias)?;
        context = new_context;

        let Some(functions) = node.get_functions().as_inner() else {
            AnalysisError::raise_unexpected_none("RangeFunction.functions")?
        };
        let mut sem_functions = Vec::new();
        for function in functions {
            let (fun, new_context) =
                DataSourceRowsFromFunc::analyze_node(context, with_clause, from_clause, function)?;
            context = new_context;
            sem_functions.push(fun);
        }
        let rf = DataSourceRowFunctions::new(&sem_functions.into(), &alias);

        Ok((Self::RowFunctions(rf), context))
    }

    fn analyze_range_func<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeFunction,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if node.get_is_rowsfrom() {
            return Self::analyze_range_func_rows_from(context, with_clause, from_clause, node);
        }

        let Some(functions) = node
            .get_functions()
            .map_values(|v| v.as_list().get_items().as_inner())
        else {
            AnalysisError::raise_unexpected_none("RangeFunction.functions")?
        };
        let functions: Vec<_> = functions
            .iter()
            .flatten()
            .filter_map(|v| v.as_inner())
            .collect();
        if functions.len() != 1 {
            AnalysisError::raise_unexpected_input(&format!(
                "RangeFunction.functions len!=1 : {functions:?}"
            ))?
        };
        let function = &functions[0];

        let Some(call) = function.as_func_call().as_inner() else {
            AnalysisError::raise_unexpected_input(&format!(
                "RangeFunction.functions[0] is not FuncCall : {functions:#?}"
            ))?
        };

        Self::call_table_func(
            context,
            with_clause,
            from_clause,
            call,
            node.get_alias().as_inner(),
        )
    }

    /// semantic analyze from clause items
    pub fn analyze_from_clause_node<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::Node,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(range_var) = node.as_range_var().as_inner() {
            Ok(Self::analyze_range_var_node(
                context,
                with_clause,
                range_var,
            )?)
        } else if let Some(range_subq) = node.as_range_subselect().as_inner() {
            Ok(Self::analyze_range_subselect(context, range_subq)?)
        } else if let Some(join_expr) = node.as_join_expr().as_inner() {
            Ok(Self::analyze_join_expr(
                context,
                with_clause,
                from_clause,
                join_expr,
            )?)
        } else if let Some(range_tablef) = node.as_range_table_func().as_inner() {
            Ok(Self::analyze_range_table_func(
                context,
                with_clause,
                from_clause,
                range_tablef,
            )?)
        } else if let Some(range_fun) = node.as_range_function().as_inner() {
            Ok(Self::analyze_range_func(
                context,
                with_clause,
                from_clause,
                range_fun,
            )?)
        } else if let Some(_range_samp) = node.as_range_table_sample().as_inner() {
            todo!()
        } else {
            AnalysisError::raise_unexpected_none("from_clause inner node")?
        }
    }
}
