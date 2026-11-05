use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AnalysisError, AnalysisProblem, CreateTrigger, DataSourceRowsFromFunc, SelectStatement,
        SemScalarExpr, WithClause,
        data_source::{
            AliasSpec, CteDataSource, DataSource, DataSourceRowFunctions, JoinExprDataSource,
            JoinKind, SubQueryDataSource, TableDataSource, UndefinedTableFuncDataSource,
            ViewDataSource, join_expr::JoinUsingClause,
        },
        problem::DeleteProblemKind,
        search_path::expand_relation_search_path,
        type_system::ArgumentBindingCollection,
    },
};

#[cfg(test)]
use crate::{
    ast::{FromItem, Relation},
    sem::FromClause as SemFromClause,
    syn::RangeVar,
};

#[cfg(test)]
impl DataSource {
    pub(super) fn analyze_range_var_node<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        range_var: RangeVar,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = FromItem::relation(Relation::try_from(&range_var)?);
        let outcome = Self::from_ast_source(
            &syntax,
            &mut context,
            with_clause,
            &SemFromClause::default(),
        );
        Ok((outcome?, context))
    }
}

impl DataSource {
    pub(crate) fn canonical_relation_column(
        &self,
        context: &impl ParseContext,
        column: &ColumnName,
    ) -> Option<ColumnName> {
        let target = match self {
            Self::Table(table) => table.name().full_name(),
            Self::View(view) => view.view_name().full_name(),
            _ => return None,
        };
        CreateTrigger::column(context, target, column)?
            .name()
            .clone()
    }

    pub(crate) fn resolved_rows(functions: &[DataSourceRowsFromFunc], alias: &AliasSpec) -> Self {
        Self::RowFunctions(DataSourceRowFunctions::new(
            &functions.to_vec().into(),
            alias,
        ))
    }
    pub(crate) fn resolve_relation<C: ParseContext>(
        context: &mut C,
        name: &FullName,
        alias: &AliasSpec,
        with: &WithClause,
    ) -> Result<Self, AnalysisError> {
        if let Some(cte_name) = with.resolve_name(context, name) {
            let cte = with.get(cte_name).expect("resolved CTE name is stored");
            if cte.is_unreadable_delete() {
                context.report_analysis_problem(AnalysisProblem::delete(
                    DeleteProblemKind::unreadable_cte(cte_name),
                ))?;
            } else {
                cte.require_readable()?;
            }
            return Ok(CteDataSource::new_data_source(cte_name, alias));
        }
        let canonical;
        let name = if context.dialect() == Dialect::sqlite() {
            canonical = expand_relation_search_path(context, name, |key| {
                context.sqlite_relation_identity(key)
            });
            canonical.as_ref().unwrap_or(name)
        } else {
            name
        };
        if let Some(name) = context.resolve_relation_name(name) {
            Ok(Self::from_relation_name(context, &name, alias))
        } else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(
                &TableName::from(name.clone()),
            ))?;
            Ok(Self::unknown(&name.to_string(), alias))
        }
    }
}

#[cfg(test)]
mod test_analyze_range_var_node_table {
    use crate::{
        ast::CreateTableMode,
        names::{ColumnName, FullName, SchemaName, TableName},
        sem::{
            CreateTable, CreateView, DataSource, FromClause, ScalarConstExpr, SelectStatement,
            SemScalarExpr, SourcePathWithSpan, ViewLike, WithClause,
            select_statement::{ResultColumnCollection, SelectStatementNode},
        },
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_var_node_table(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        let with_clause = WithClause::default();
        let builder = SynBuilder::new();
        let name = FullName::from_local_name("table1");
        let alias = builder.alias("t1", &[]);
        let syn = builder.range_var(&name, &alias.into());

        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        let table_full_name = FullName::with_schema("public", "table1");
        let table_definition = CreateTable::new(
            &TableName::from(table_full_name.clone()),
            vec![],
            CreateTableMode::Default,
            false,
            &source_span,
        );
        test_context.set_get_table_impl_result(&table_full_name, &Some(table_definition));

        let (_ds, _context) = DataSource::analyze_range_var_node(test_context, &with_clause, syn)?;

        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_range_var_node_view(mut test_context: TestParseContext) -> TestResult {
        let builder = SynBuilder::new();
        let name = FullName::from_local_name("view1");
        let alias = builder.alias("v1", &[]);
        let syn = builder.range_var(&name, &alias.into());

        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        // CREATE VIEW public.view1 as select 10 as col;
        let table_full_name = FullName::with_schema("public", "view1");
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut result_columns = ResultColumnCollection::default();
        let value_expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));
        result_columns.push_column(&ColumnName::from("col"), &value_expr);
        let simple_query =
            SelectStatementNode::new(&result_columns, &with_clause, &from_clause, &None);
        let query = SelectStatement::simple(&simple_query);
        let create_view = CreateView::new(&table_full_name, &query, &[], &None, false);
        let view_def = ViewLike::from(&create_view);
        test_context.set_get_table_impl_result(&table_full_name, &None);
        test_context.set_get_view_impl_result(&table_full_name, &Some(view_def));

        let (_ds, _context) = DataSource::analyze_range_var_node(test_context, &with_clause, syn)?;

        Ok(())
    }
}

#[cfg(test)]
mod test_action_lookup_retains_the_first_relation_on_the_search_path {
    use crate::{
        names::FullName,
        sem::{CreateView, ViewLike},
        test_helpers::{rule as test_helpers, statements},
    };

    #[test]
    fn action_lookup_retains_the_first_relation_on_the_search_path() -> testresult::TestResult {
        let mut context = test_helpers::catalog();
        let query = statements::resolve_postgres("SELECT id FROM public.sink", &mut context)?
            .into_select_statemtnt()
            .unwrap();
        let name = FullName::with_schema("pg_catalog", "items");
        let view = CreateView::new(&name, &query, &[], &None, false);
        context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
        let statement = statements::resolve_postgres(
            "CREATE RULE r AS ON UPDATE TO public.sink DO SELECT id FROM items",
            &mut context,
        )?;
        let rule = statement.rule().unwrap();
        assert!(rule.dependencies().relations().contains(&name));
        assert!(
            !rule
                .dependencies()
                .relations()
                .contains(&FullName::with_schema("public", "items"))
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_sqlite_cte_precedes_case_insensitive_catalog_lookup {

    use crate::{ast::Dialect, test_helpers::trigger as test_helpers};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::query_cte(
        "WITH Items AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM ITEMS",
        "Items"
    )]
    #[case::qualified_table(
        "WITH Items AS (SELECT 7 AS cte_only) SELECT id AS picked FROM main.items",
        "main.items"
    )]
    #[case::nested_cte_shadow(
        "WITH Items AS (SELECT 7 AS outer_only) SELECT picked FROM (WITH items AS (SELECT 9 AS inner_only) SELECT inner_only AS picked FROM ITEMS) s",
        "items"
    )]
    #[case::trigger_cte(
        "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT picked FROM (WITH Other AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM OTHER) s; END",
        "Other"
    )]
    #[case::trigger_qualified_table(
        "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT picked FROM (WITH Other AS (SELECT 7 AS cte_only) SELECT id AS picked FROM main.other) s; END",
        "main.other"
    )]
    fn sqlite_cte_precedes_case_insensitive_catalog_lookup(
        #[case] sql: &str,
        #[case] expected_source: &str,
    ) -> testresult::TestResult {
        let oracle = test_helpers::sqlite_oracle()?;
        oracle.execute_batch("INSERT INTO items VALUES (42); INSERT INTO other VALUES (42);")?;
        let mut context = test_helpers::catalog(Dialect::sqlite());
        let statement = test_helpers::resolve(sql, &mut context)?;
        let command = if let Some(trigger) = statement.trigger() {
            oracle.execute_batch(sql)?;
            trigger.body().statements()[0].clone()
        } else {
            let actual: i64 = oracle.query_row(sql, [], |row| row.get(0))?;
            assert_eq!(
                actual,
                match expected_source {
                    "Items" => 7,
                    "items" => 9,
                    _ => 42,
                }
            );
            statement
        };
        let mut query = command.into_select_statemtnt().unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(
            query
                .get_result_column_def(&crate::names::ColumnName::from("picked"))
                .unwrap()
                .get_type_cloned()
                .is_some()
        );
        assert_eq!(test_helpers::bound_sources(&mut query)?, [expected_source]);
        Ok(())
    }

    #[rstest::rstest]
    #[case::quoted_cte(
        r#"WITH "Items" AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM "Items""#,
        "Items"
    )]
    #[case::case_sensitive_catalog_fallback(
        r#"WITH "Items" AS (SELECT 7 AS cte_only) SELECT id AS picked FROM ITEMS"#,
        "public.items"
    )]
    fn postgres_cte_lookup_preserves_quoted_case(
        #[case] sql: &str,
        #[case] expected_source: &str,
    ) -> testresult::TestResult {
        let mut context = test_helpers::catalog(Dialect::postgres());
        let mut query = test_helpers::resolve(sql, &mut context)?
            .into_select_statemtnt()
            .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(test_helpers::bound_sources(&mut query)?, [expected_source]);
        Ok(())
    }
}

impl DataSource {
    fn from_relation_name(context: &impl ParseContext, name: &FullName, alias: &AliasSpec) -> Self {
        if context.get_table(&TableName::from(name.clone())).is_some() {
            TableDataSource::new_data_source(&TableName::from(name.clone()), alias)
        } else {
            Self::View(ViewDataSource::from_name(
                &ViewName::from_full_name(name),
                alias,
            ))
        }
    }

    pub(crate) fn resolved_query(query: &SelectStatement, alias: &AliasSpec) -> Self {
        SubQueryDataSource::new_data_source(query, alias)
    }

    pub(crate) fn resolved_join(
        left: &Self,
        right: &Self,
        kind: &JoinKind,
        condition: &Option<SemScalarExpr>,
        alias: &Option<AliasSpec>,
        using: &Option<JoinUsingClause>,
        natural: bool,
    ) -> Self {
        JoinExprDataSource::new_data_source(kind, left, right, condition, alias, using, natural)
    }

    pub(crate) fn resolve_function<C: ParseContext>(
        context: &mut C,
        name: &FullName,
        arguments: &[SemScalarExpr],
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        let arg_types = ArgumentBindingCollection::from_expr_list(arguments);
        let Some(overloads) = context.get_function_by_name(name) else {
            context.report_analysis_problem(AnalysisProblem::table_function_not_found(name))?;
            return Ok(Self::UndefinedTableFunc(
                UndefinedTableFuncDataSource::from_name(name, arguments, alias),
            ));
        };
        let Some(overload) = overloads.resolve_from_catalog(context, &arg_types) else {
            context.report_analysis_problem(
                AnalysisProblem::parameter_type_mismatch_for_table_function(name, &arg_types),
            )?;
            return Ok(Self::UndefinedTableFunc(
                UndefinedTableFuncDataSource::from_name(name, arguments, alias),
            ));
        };
        let mut source = Self::table_function(&overload, arguments.to_vec().into(), alias);
        source.record_function_name(name, &context.resolve_function_name_with_search_path(name));
        Ok(source)
    }
}
