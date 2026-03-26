mod impl_analyze_range_var_node;

use sql_fun_core::IVec;

use crate::{
    AstAndContextPair,
    sem::{
        AliasSpec, AnalysisError, ColumnName, DataSourceRowFunctions, DataSourceRowsFromFunc,
        FromClause, FullName, JoinKind, ParseContext, SemAst, SemScalarExpr, WithClause,
        data_source::{JoinExprDataSource, join_expr::JoinUsingClause},
    },
    syn::{ListOpt, Opt, ScanToken},
};

use super::{DataSource, SubQueryDataSource};

impl DataSource {
    // subquery can not to use CTEs, not need to with_clause
    fn analyze_range_subselect<TParseContext>(
        mut context: TParseContext,
        range_subsel: crate::syn::RangeSubselect,
        tokens: &IVec<ScanToken>,
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
        let AstAndContextPair(sem, new_context) =
            SemAst::analyze_node(context, &None, query, tokens)?;
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
    use sql_fun_core::IVec;
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
        let tokens = IVec::default();

        super::DataSource::analyze_range_subselect(test_context, range_subselect, &tokens)?;
        Ok(())
    }
}

impl DataSource {
    fn analyze_join_using_clause<TParseContext>(
        mut context: TParseContext,
        join_expr: crate::syn::JoinExpr,
    ) -> Result<(Option<JoinUsingClause>, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(using_columns) = join_expr.get_using_clause().as_inner()
            && !using_columns.is_empty()
        {
            let using_columns: Vec<ColumnName> = using_columns
                .iter()
                .filter_map(|c| c.as_string().get_sval())
                .map(|v| ColumnName::from(v.as_str()))
                .collect();

            let (alias, new_context) =
                if let Some(alias) = join_expr.get_join_using_alias().as_inner() {
                    let (alias, new_context) = AliasSpec::analyze_without_relname(context, &alias)?;
                    (Some(alias), new_context)
                } else {
                    (None, context)
                };

            context = new_context;
            let join_using = JoinUsingClause::new(&using_columns, &alias);
            Ok((Some(join_using), context))
        } else {
            Ok((None, context))
        }
    }

    fn analyze_join_expr<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        join_expr: crate::syn::JoinExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut from_clause = from_clause.clone();

        let Some(left) = join_expr.get_larg().as_inner() else {
            AnalysisError::raise_unexpected_none("join_expr.larg")?
        };
        let Some(right) = join_expr.get_rarg().as_inner() else {
            AnalysisError::raise_unexpected_none("join_expr.rarg")?
        };

        let (left_source, new_context) =
            Self::analyze_from_clause_node(context, with_clause, &from_clause, left, tokens)?;
        context = new_context;
        let (right_source, new_context) =
            Self::analyze_from_clause_node(context, with_clause, &from_clause, right, tokens)?;
        context = new_context;

        from_clause.push(left_source.clone());
        from_clause.push(right_source.clone());

        let qualifications = if let Some(qualifications) = join_expr.get_quals().as_inner() {
            let (quals, new_context) = SemScalarExpr::analyze_scaler_expr(
                context,
                with_clause,
                &from_clause,
                qualifications,
                tokens,
            )?;
            context = new_context;
            Some(quals)
        } else {
            None
        };

        let alias = if let Some(alias) = join_expr.get_alias().as_inner() {
            let (alias, new_context) = AliasSpec::analyze_without_relname(context, &alias)?;
            context = new_context;
            Some(alias)
        } else {
            None
        };

        let join_kind = JoinKind::try_from(join_expr.get_jointype())?;
        let natural = join_expr.get_is_natural();
        let (using_clause, new_context) = Self::analyze_join_using_clause(context, join_expr)?;
        context = new_context;

        Ok((
            JoinExprDataSource::new_data_source(
                &join_kind,
                &left_source,
                &right_source,
                &qualifications,
                &alias,
                &using_clause,
                natural,
            ),
            context,
        ))
    }
}

#[cfg(test)]
mod test_analyze_join_expr {
    use crate::{
        sem::{
            ColumnDefinition, ColumnName, FromClause, FullName, SchemaName, TypeReference,
            WithClause,
        },
        syn::{AliasOpt, JoinType, NodeInner},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_join_expr(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&vec![
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let int4_type_ref = TypeReference::from_master_type_name("int4");
        test_context.setup_type(int4_type_ref.clone());
        let columns = vec![ColumnDefinition::new(
            &Some(ColumnName::from("id")),
            Some(&int4_type_ref),
            Some(true),
            &[],
        )];
        test_context.setup_table("table1", &columns);
        test_context.setup_table("table2", &columns);
        test_context.setup_binary("=", "int4", "int4", "bool", true);

        let builder = SynBuilder::new();

        let name = FullName::from_local_name("table1");
        let alias = builder.alias("t1", &[]);
        let table1 = NodeInner::RangeVar(builder.range_var(&name, &alias.into()));
        let name = FullName::from_local_name("table2");
        let alias = builder.alias("t2", &[]);
        let table2 = NodeInner::RangeVar(builder.range_var(&name, &alias.into()));
        let left = builder.column_ref("t1", "id");
        let right = builder.column_ref("t2", "id");
        let qual = builder.operator_equals(
            NodeInner::ColumnRef(left).into(),
            NodeInner::ColumnRef(right).into(),
        );

        let join_expr = builder.join_expr(
            JoinType::JoinInner.into(),
            table1.into(),
            table2.into(),
            qual,
            AliasOpt::none(),
            &[],
            AliasOpt::none(),
            false,
        );

        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let tokens = IVec::default();

        let (_ds, _) = super::DataSource::analyze_join_expr(
            test_context,
            &with_clause,
            &from_clause,
            join_expr,
            &tokens,
        )?;

        Ok(())
    }
}

impl DataSource {
    fn analyze_range_table_func<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeTableFunc,
        tokens: &IVec<ScanToken>,
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
            tokens,
        )
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_table_func {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use crate::{
        sem::{DataSource, SchemaName, WithClause},
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_range_table_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&vec![SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("table_func", &[overload.clone()]);

        let builder = SynBuilder::new();
        let func_call_node = builder.build_func_call_node("table_func");
        let alias = builder.alias("func_alias", &[]);
        let range_table_func = builder.build_range_table_func(func_call_node, alias);
        let tokens = IVec::default();
        let with_clause = WithClause::default();
        let from_clause = super::FromClause::default();

        let (ds, _) = super::DataSource::analyze_range_table_func(
            test_context,
            &with_clause,
            &from_clause,
            range_table_func,
            &tokens,
        )?;

        let mut aliases = Vec::new();
        ds.push_alias_names(&mut aliases);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name().as_str(), "func_alias");
        let DataSource::CallTableFunc(table_func_ds) = ds else {
            panic!("expected table function datasource");
        };
        assert_eq!(table_func_ds.function(), &overload);
        Ok(())
    }
}

impl DataSource {
    fn analyze_range_func_rows_from<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeFunction,
        tokens: &IVec<ScanToken>,
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
            let (fun, new_context) = DataSourceRowsFromFunc::analyze_node(
                context,
                with_clause,
                from_clause,
                function,
                tokens,
            )?;
            context = new_context;
            sem_functions.push(fun);
        }
        let rf = DataSourceRowFunctions::new(&sem_functions.into(), &alias);

        Ok((Self::RowFunctions(rf), context))
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_func_rows_from {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use crate::{
        sem::{
            AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, SchemaName,
            WithClause,
        },
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_range_func_rows_from(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&vec![SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        let func_name = test_context.setup_function("rows_func", &[overload.clone()]);

        let builder = SynBuilder::new();
        let func_call_node = builder.build_func_call_node(func_name.local_name().as_str());
        let range_function = builder.build_range_function(func_call_node, "rows_alias", true);
        let tokens = IVec::default();
        let with_clause = WithClause::default();
        let from_clause = super::FromClause::default();

        let (ds, _) = super::DataSource::analyze_range_func_rows_from(
            test_context,
            &with_clause,
            &from_clause,
            range_function,
            &tokens,
        )?;

        let expected_rows_func = DataSourceRowsFromFunc::FuncCall(
            func_name.clone(),
            Vec::new(),
            overload.returning_table(),
        );
        let expected_functions: IVec<_> = vec![expected_rows_func].into();
        let expected_alias = AliasSpec::from_local_name("rows_alias");
        let expected_row_functions =
            DataSourceRowFunctions::new(&expected_functions, &expected_alias);
        let expected = DataSource::RowFunctions(expected_row_functions);
        assert_eq!(expected, ds);
        Ok(())
    }
}

impl DataSource {
    fn analyze_range_func<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeFunction,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if node.get_is_rowsfrom() {
            return Self::analyze_range_func_rows_from(
                context,
                with_clause,
                from_clause,
                node,
                tokens,
            );
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
            .filter_map(crate::syn::Opt::as_inner)
            .collect();
        if functions.len() != 1 {
            AnalysisError::raise_unexpected_input(&format!(
                "RangeFunction.functions len!=1 : {functions:?}"
            ))?;
        }
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
            tokens,
        )
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_functions {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use crate::{
        sem::{DataSource, SchemaName, WithClause},
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_range_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&vec![SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("set_func", &[overload.clone()]);

        let builder = SynBuilder::new();
        let func_call_node = builder.build_func_call_node("set_func");
        let functions_node = builder.build_list_node(vec![func_call_node]);
        let range_function = builder.build_range_function(functions_node, "set_alias", false);
        let tokens = IVec::default();
        let with_clause = WithClause::default();
        let from_clause = super::FromClause::default();

        let (ds, _) = super::DataSource::analyze_range_func(
            test_context,
            &with_clause,
            &from_clause,
            range_function,
            &tokens,
        )?;

        let mut aliases = Vec::new();
        ds.push_alias_names(&mut aliases);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name().as_str(), "set_alias");
        let DataSource::CallTableFunc(table_func) = ds else {
            panic!("expected table function datasource");
        };
        assert_eq!(table_func.function(), &overload);
        Ok(())
    }
}

impl DataSource {
    /// semantic analyze from clause items
    pub fn analyze_from_clause_node<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::Node,
        tokens: &IVec<ScanToken>,
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
            Ok(Self::analyze_range_subselect(context, range_subq, tokens)?)
        } else if let Some(join_expr) = node.as_join_expr().as_inner() {
            Ok(Self::analyze_join_expr(
                context,
                with_clause,
                from_clause,
                join_expr,
                tokens,
            )?)
        } else if let Some(range_tablef) = node.as_range_table_func().as_inner() {
            Ok(Self::analyze_range_table_func(
                context,
                with_clause,
                from_clause,
                range_tablef,
                tokens,
            )?)
        } else if let Some(range_fun) = node.as_range_function().as_inner() {
            Ok(Self::analyze_range_func(
                context,
                with_clause,
                from_clause,
                range_fun,
                tokens,
            )?)
        } else if let Some(_range_samp) = node.as_range_table_sample().as_inner() {
            todo!()
        } else {
            AnalysisError::raise_unexpected_none("from_clause inner node")?
        }
    }
}
