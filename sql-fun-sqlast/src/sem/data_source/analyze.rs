#[cfg(test)]
mod impl_analyze_range_var_node;

use crate::ast::TryFromWithTokens;
use crate::sem::data_source::DataSource;
use crate::{
    sem::{AnalysisError, FromClause, ParseContext, WithClause},
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

impl DataSource {
    // Standalone boundary fixture; nested queries resolve through their common scope.
    #[cfg(test)]
    fn analyze_range_subselect<TParseContext>(
        mut context: TParseContext,
        range_subsel: crate::syn::RangeSubselect,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::FromItem::query(crate::ast::DerivedTable::try_from_with_tokens(
            &range_subsel,
            tokens,
        )?);
        let outcome =
            syntax.resolve_source(&mut context, &WithClause::default(), &FromClause::default());
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_analyze_range_subselect {
    use crate::{
        sem::{MutSourcePathWithSpan, SourcePathWithSpan},
        syn::{Node, NodeList, WithClauseOpt},
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_subselect(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        let builder = SynBuilder::new();
        test_context.set_source_path_with_span(&source_span);

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
    #[cfg(test)]
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
        let syntax =
            crate::ast::FromItem::join(crate::ast::Join::try_from_with_tokens(&join_expr, tokens)?);
        let outcome = syntax.resolve_source(&mut context, with_clause, from_clause);
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_analyze_join_expr {
    use crate::{
        sem::{
            ColumnDefinition, ColumnName, FromClause, FullName, SchemaName, SourcePathWithSpan,
            TypeReference, WithClause,
        },
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_join_expr(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[
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
        test_context.setup_table("table1", &columns, &source_span);
        test_context.setup_table("table2", &columns, &source_span);
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

        let join_expr = builder.inner_join(&table1.into(), &table2.into(), &qual);

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
    #[cfg(test)]
    fn analyze_range_table_func<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeTableFunc,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax =
            crate::ast::FromItem::xml(crate::ast::XmlTable::try_from_with_tokens(&node, tokens)?);
        let outcome = syntax.resolve_source(&mut context, with_clause, from_clause);
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_table_func {
    use crate::{
        sem::{DataSource, SchemaName, WithClause},
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_table_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("table_func", std::slice::from_ref(&overload));

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
    #[cfg(test)]
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
        let syntax = crate::ast::FromItem::functions(
            crate::ast::TableFunctions::try_from_with_tokens(&node, tokens)?,
        );
        let outcome = syntax.resolve_source(&mut context, with_clause, from_clause);
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_func_rows_from {
    use crate::{
        sem::{
            AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, SchemaName,
            WithClause,
        },
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_func_rows_from(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        let func_name = test_context.setup_function("rows_func", std::slice::from_ref(&overload));

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
        let expected_alias = AliasSpec::from_names("rows_alias", &[]);
        let expected_row_functions =
            DataSourceRowFunctions::new(&expected_functions, &expected_alias);
        let expected = DataSource::RowFunctions(expected_row_functions);
        assert_eq!(expected, ds);
        Ok(())
    }
}

impl DataSource {
    #[cfg(test)]
    fn analyze_range_func<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::RangeFunction,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::FromItem::functions(
            crate::ast::TableFunctions::try_from_with_tokens(&node, tokens)?,
        );
        let outcome = syntax.resolve_source(&mut context, with_clause, from_clause);
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_functions {
    use crate::{
        sem::{DataSource, SchemaName, WithClause},
        test_helpers::{SynBuilder, TestParseContext, setup_function_overload, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("set_func", std::slice::from_ref(&overload));

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
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        node: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::FromItem::try_from_with_tokens(&node, tokens)?;
        let result = syntax.resolve_source(&mut context, with_clause, from_clause);
        Ok((result?, context))
    }
}
