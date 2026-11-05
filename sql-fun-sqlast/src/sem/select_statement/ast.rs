use crate::{
    CacheContext,
    ast::{Dialect, Expr, Query, QueryBody, Select, SetOperation, SetQuery},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CteStatement, DataSource, FromClause, QueryGrouping,
        QueryLock, QueryOrder, QueryWindow, ResolutionFailure, ResultColumnCollection,
        RuleIdentity, ScalarAnalysisContext, SelectClauses, SelectDestination, SelectStatement,
        SelectStatementNode, SemScalarExpr, SemanticQuery, SetOperationKind, WithClause,
        problem::RuleProblem, query::ClauseBinding, with_stack,
    },
};

impl SelectStatement {
    #[cfg(test)]
    pub(crate) fn from_ast_test_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_in(ast, context, outer)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast(
        ast: &Query,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None);
        let result = context.infer_query_parameter_types(result, cache);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_parameter_analysis_lifecycle {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{
            PgBuiltInType,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::public_analysis::analyze,
    };

    #[rstest::rstest]
    #[case::query("with x as (select 1 as n) select n + 1 from x")]
    #[case::subquery("select (select 1) + 1 as n")]
    fn parameter_free_analysis_skips_inference(#[case] sql: &str) {
        let before = parameter_resolution_inference_runs();
        let result = analyze(sql, Dialect::postgres()).unwrap();
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_none()
        );
        assert!(
            result
                .context()
                .parameter_constraints()
                .result_column(0)
                .is_none()
        );
    }

    #[test]
    fn parameter_free_function_keeps_null_argument_resolution() {
        let before = parameter_resolution_inference_runs();
        let result = analyze("select lower(null) as s", Dialect::postgres()).unwrap();
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert_eq!(result.context().reported_problem_count(), 0);
        let query = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::text())
        );
    }

    #[test]
    fn inference_preserves_unrelated_name_resolution_diagnostics() {
        let baseline = analyze("select missing_column as absent", Dialect::postgres()).unwrap();
        let expected = baseline.context().reported_problems();
        assert!(!expected.is_empty());
        let before = parameter_resolution_inference_runs();
        let result = analyze(
            "select $1 + 1 as n, missing_column as absent",
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert_eq!(
            format!("{:?}", result.context().reported_problems()),
            format!("{expected:?}")
        );
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_insert_cte_parameter_analysis {
    use crate::{
        ast::Dialect, context::ParseContext,
        sem::parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        test_helpers::insert::analyze,
    };

    #[test]
    fn unreferenced_modifying_cte_is_still_analyzed_once() {
        let before = parameter_resolution_inference_runs();
        let (_, context) = analyze(
            "WITH changed AS (INSERT INTO items(id) VALUES($1)) SELECT 1",
            Dialect::postgres(),
        );
        assert_eq!(context.reported_problem_count(), 0);
        assert!(context.parameter_constraints().parameter(1).is_some());
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
    }
}

#[cfg(test)]
mod test_update_cte_parameter_analysis {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::{
            SemAst, parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{create_cache_context, update::catalog},
    };

    #[test]
    fn unreferenced_update_cte_is_analyzed_in_one_inference_pass() {
        let mut context = catalog(Dialect::postgres());
        let statement = parse_statements(
            "WITH u AS (UPDATE items SET id=$1) SELECT 1",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        let before = parameter_resolution_inference_runs();
        SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context()).unwrap();
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert_eq!(context.reported_problem_count(), 0);
        assert!(context.parameter_constraints().parameter(1).is_some());
    }
}

#[cfg(test)]
mod test_delete_cte_parameter_analysis {

    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::ColumnName,
        sem::{
            PgBuiltInType,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{delete as test_helpers, delete::catalog},
    };

    #[test]
    fn cte_aliases_and_parameters_survive_a_single_outer_inference_pass() {
        let mut context = catalog(Dialect::postgres());
        let before = parameter_resolution_inference_runs();
        let result = test_helpers::resolve("WITH s AS (SELECT $1::integer n), d(removed) AS (DELETE FROM items USING s WHERE $2 RETURNING s.n) SELECT removed FROM d", &mut context, Dialect::postgres()).unwrap();
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let query = result.into_select_statemtnt().unwrap();
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .column_name(),
            &Some(ColumnName::from("removed"))
        );
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(2)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::bool())
        );
    }
}

#[cfg(test)]
mod test_parameter_analysis_clauses {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{
            PgBuiltInType, TypeReference as SemTypeReference,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::order("SELECT 1 ORDER BY $7 + 1", 7, PgBuiltInType::int4())]
    #[case::group("SELECT 1 GROUP BY $7 + 1", 7, PgBuiltInType::int4())]
    #[case::having("SELECT 1 HAVING $7", 7, PgBuiltInType::bool())]
    #[case::limit("SELECT 1 LIMIT $7", 7, PgBuiltInType::int8())]
    #[case::offset("SELECT 1 OFFSET $7", 7, PgBuiltInType::int8())]
    #[case::distinct("SELECT DISTINCT ON ($7 + 1) 1", 7, PgBuiltInType::int4())]
    #[case::window_partition("SELECT sum(1) OVER (PARTITION BY $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::window_order(
        "SELECT sum(1) OVER w WINDOW w AS (ORDER BY $7 + 1)",
        7,
        PgBuiltInType::int4()
    )]
    #[case::frame(
        "SELECT sum(1) OVER (ORDER BY 1 ROWS $7 PRECEDING)",
        7,
        PgBuiltInType::int8()
    )]
    #[case::filter("SELECT sum(1) FILTER (WHERE $7)", 7, PgBuiltInType::bool())]
    #[case::aggregate_order("SELECT sum(1 ORDER BY $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::within_group(
        "SELECT mode() WITHIN GROUP (ORDER BY $7 + 1)",
        7,
        PgBuiltInType::int4()
    )]
    #[case::nested_order("SELECT 1 ORDER BY (SELECT $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::compound_limit("SELECT 1 UNION ALL SELECT 2 LIMIT $7", 7, PgBuiltInType::int8())]
    #[case::operand_limit("(SELECT 1 LIMIT $7) UNION ALL SELECT 2", 7, PgBuiltInType::int8())]
    #[case::values("VALUES (1), ($7 + 1)", 7, PgBuiltInType::int4())]
    fn infers_parameters_outside_projection(
        #[case] sql: &str,
        #[case] position: u32,
        #[case] ty: SemTypeReference,
    ) {
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{sql}: {:?}",
            context.reported_problems()
        );
        let parameters = context.parameter_constraints();
        let domain = parameters
            .parameter(position)
            .expect("retained numbered parameter")
            .constraint();
        assert!(!domain.is_any());
        assert!(domain.allows(&ty), "{sql}: {domain:?}");
        assert!(!domain.allows(&PgBuiltInType::text()));
    }

    #[rstest::rstest]
    #[case::clauses("SELECT 1 AS n GROUP BY n HAVING true ORDER BY n LIMIT 1 OFFSET 0")]
    #[case::nested("WITH a AS (SELECT 1 AS n ORDER BY n) SELECT n FROM a ORDER BY (SELECT 1)")]
    #[case::compound("SELECT 1 UNION ALL SELECT 2 ORDER BY 1 LIMIT 1")]
    fn parameter_free_queries_skip_inference(#[case] sql: &str) {
        let before = parameter_resolution_inference_runs();
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(context.parameter_constraints().parameter(1).is_none());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::order("SELECT 1 ORDER BY CAST(?7 AS INTEGER)")]
    #[case::group("SELECT 1 GROUP BY CAST(?7 AS INTEGER)")]
    #[case::having("SELECT 1 GROUP BY 1 HAVING ?7 IS NULL")]
    #[case::limit("SELECT 1 LIMIT ?7")]
    #[case::offset("SELECT 1 LIMIT 1 OFFSET ?7")]
    #[case::compound("SELECT 1 UNION ALL SELECT 2 LIMIT ?7")]
    fn sqlite_retains_clause_parameters(#[case] sql: &str) {
        let (_, context) = resolve(sql, Dialect::sqlite(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert!(context.parameter_constraints().parameter(7).is_some());
    }

    #[test]
    fn parameter_conflicts_across_clauses_produce_diagnostics() {
        let (_, context) =
            resolve("SELECT 1 HAVING $1 LIMIT $1", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problems().len(), 1);
        assert_eq!(
            context.reported_problems()[0].code(),
            "parameter_constraint"
        );
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_in_body(ast, context, outer))
    }
}

impl SelectStatement {
    #[inline(never)]
    pub(crate) fn from_ast_in_body(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::analyze_ast_query(ast, context, outer)
    }
}

impl SelectStatement {
    pub(crate) fn analyze_ast_query(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let with = WithClause::from_ast(ast.with(), context, outer)?;
        let restricted = if Self::ast_is_compound(ast.body())
            && outer.is_some_and(|scope| scope.direct_rule_identity().is_some())
        {
            outer.map(ScalarAnalysisContext::without_rule_rows)
        } else {
            None
        };
        let query_id = context.analysis_state_mut().grouping_mut().new_query();
        let body = Self::from_ast_body(
            ast.body(),
            context,
            &with,
            restricted.as_ref().or(outer),
            query_id,
        )?;
        if body.select_clauses().is_none() {
            context.analysis_state_mut().grouping_mut().set_keys(
                query_id,
                &[],
                body.result_columns(),
            )?;
        }
        let empty = FromClause::default();
        let order_outer = if Self::ast_is_compound(ast.body()) {
            None
        } else {
            outer
        };
        let mut scope = ScalarAnalysisContext::nested(
            &with,
            body.source_scope().unwrap_or(&empty),
            order_outer,
        );
        scope.set_grouping_query(query_id);
        scope.set_windows(Self::ast_window_names(ast.body()));
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(body.result_columns());
        }
        let binding = QueryOrder::ast_order_binding(ast.body());
        let order = QueryOrder::from_ast_for_query(
            ast.order(),
            ast.body(),
            context,
            &scope,
            body.result_columns(),
            binding,
        )?;
        let row_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let row_scope = row_scope.with_grouping_placement("LIMIT/OFFSET");
        let limit = SemScalarExpr::from_ast_optional(ast.limit(), context, &row_scope)?;
        let offset = SemScalarExpr::from_ast_optional(ast.offset(), context, &row_scope)?;
        let locks = QueryLock::from_ast_list(ast.locks(), context, &scope)?;
        let local = WithClause::ast_local_definitions(ast.with(), with);
        let mut query =
            SemanticQuery::new(local, ast.with().recursive(), body, order, limit, offset);
        query.set_modifiers(ast.with_ties(), Self::ast_set_all(ast.body()), locks);
        let mut query = Self::query(query);
        query.complete_grouping(context)?;
        Ok(query)
    }
}

#[cfg(test)]
mod test_retains_query_and_select_structure {
    use crate::{
        sem::{QueryGrouping, QueryNameResolution, SelectStatement},
        test_helpers::query::{output_index, query},
    };

    #[test]
    fn retains_query_and_select_structure() {
        let query = query(
            "SELECT DISTINCT ON (n) n FROM (SELECT 1 AS n) s GROUP BY DISTINCT GROUPING SETS ((n), ()) HAVING true WINDOW w AS (ORDER BY n) ORDER BY n DESC NULLS FIRST FETCH FIRST 2 ROWS WITH TIES",
        );
        let query = query.query_clauses().unwrap();
        assert!(query.with_ties());
        let order = &query.order()[0];
        assert_eq!(order.descending(), Some(true));
        assert_eq!(order.nulls_first(), Some(true));
        assert_eq!(output_index(order.expression()), Some(0));
        let select = query.body().select_clauses().unwrap();
        assert_eq!(select.distinct().unwrap().len(), 1);
        assert!(select.group_distinct());
        assert_matches::assert_matches!(&select.group()[0], QueryGrouping::Sets(groups) if groups.len() == 2);
        assert!(select.having().is_some());
        assert_eq!(select.windows()[0].name(), Some("w"));
    }

    #[test]
    fn compound_boundary_owns_its_trailing_clauses() {
        let query = query(
            "(SELECT 1 AS n LIMIT 1) UNION ALL (SELECT 2 LIMIT 2) ORDER BY n LIMIT 3 OFFSET 4",
        );
        let boundary = query.query_clauses().unwrap();
        assert!(boundary.set_all());
        assert_eq!(boundary.order().len(), 1);
        assert!(boundary.limit().is_some());
        assert!(boundary.offset().is_some());
        let SelectStatement::Set(set) = boundary.body() else {
            panic!("compound body");
        };
        assert!(set.left().query_clauses().unwrap().order().is_empty());
        assert!(set.right().query_clauses().unwrap().order().is_empty());
    }

    #[test]
    fn retains_declared_destination_and_lock_bindings() {
        let query =
            query("SELECT n INTO TEMP TABLE destination FROM (SELECT 1 AS n) s FOR UPDATE OF s");
        let boundary = query.query_clauses().unwrap();
        let destination = boundary
            .body()
            .select_clauses()
            .unwrap()
            .destination()
            .unwrap();
        assert_eq!(destination.name().local_name(), "destination");
        assert_eq!(destination.persistence(), "t");
        assert_matches::assert_matches!(
            boundary.locks()[0].relations()[0].resolution(),
            QueryNameResolution::SourceAlias(_)
        );
    }
}

#[cfg(test)]
mod test_untyped_operands_do_not_prevent_name_resolution {
    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        names::OperatorName,
        parse_statements,
        sem::{
            BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, SelectStatement,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case("SELECT CASE WHEN true THEN $1 ELSE 1 END")]
    #[case("SELECT COALESCE($1, 1)")]
    #[case("SELECT ($1)[$2]")]
    #[case("SELECT $1 = ANY($2)")]
    #[case("SELECT $1 = ALL($2)")]
    fn untyped_operands_do_not_prevent_name_resolution(#[case] sql: &str) {
        let dialect = Dialect::postgres();
        let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
        let StatementNode::Query(query) = statement.node() else {
            panic!("query fixture expected");
        };
        let mut catalog = TestParseContext::default();
        let int = PgBuiltInType::int4();
        let operator = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
            &int,
            &int,
            &PgBuiltInType::bool(),
            true,
        )]);
        catalog.set_get_operator_op_result(
            &OperatorName::from_parts(None, "="),
            true,
            true,
            Some(&operator),
        );
        let before = parameter_resolution_inference_runs();
        catalog.begin_parameter_type_analysis();
        let result = SelectStatement::from_ast_in(query, &mut catalog, None);
        catalog.finish_analysis(result).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_body(
        ast: &QueryBody,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        match ast {
            QueryBody::Select(select) => {
                Self::from_ast_select(select.as_ref(), context, with, outer, query_id)
            }
            QueryBody::Set(set) => Self::from_ast_set(set.as_ref(), context, with, outer),
            QueryBody::Values(rows) => Self::from_ast_values(rows, context, with, outer, query_id),
        }
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_values(
        rows: &[Vec<Expr>],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let mut scope = ScalarAnalysisContext::nested(with, &from, outer);
        scope.set_grouping_query(query_id);
        let scope = scope.with_grouping_placement("VALUES");
        let mut resolved = Vec::new();
        for row in rows {
            resolved.push(SemScalarExpr::from_ast_list(row, context, &scope)?);
        }
        Ok(Self::values(resolved))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set_operation(
        ast: &SetOperation,
        context: &mut impl ParseContext,
        left: Self,
        right: Self,
    ) -> Result<Self, AnalysisError> {
        let kind = match ast {
            SetOperation::Union => SetOperationKind::union(),
            SetOperation::Intersect => SetOperationKind::intersect(),
            SetOperation::Except => SetOperationKind::except(),
        };
        Self::set_operation(context, kind, left, right)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_select(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        Self::validate_ast_rule_destination(ast, context, outer)?;
        let from = Self::from_ast_from(ast, context, with, outer, query_id)?;
        let mut scope = Self::ast_scope(ast, with, &from, outer, query_id);
        let windows = Self::from_ast_windows(ast, context, &scope)?;
        let predicate = Self::from_ast_where(ast, context, &scope)?;
        let mut results = Self::from_ast_targets(ast, context, with, &from, &scope)?;
        let distinct = Self::from_ast_distinct(ast, context, &scope, &results)?;
        let group = Self::from_ast_grouping(ast, context, &mut scope, &mut results, query_id)?;
        let having = Self::from_ast_having(ast, context, &scope)?;
        let clauses = Self::from_ast_clauses(ast, distinct, group, having, windows)?;
        Ok(Self::ast_build_result(results, from, predicate, clauses))
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_rule_destination(
        ast: &Select,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<(), AnalysisError> {
        if ast.into().is_some()
            && let Some(scope) = outer
            && let Some(identity) = scope.rule_identity()
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity,
                "SELECT INTO is not allowed in rule actions",
            )))?;
        }
        Ok(())
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_from(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<FromClause, AnalysisError> {
        let empty = FromClause::default();
        let mut from_scope = ScalarAnalysisContext::nested(with, &empty, outer);
        from_scope.set_grouping_query(query_id);
        let from_scope = from_scope.with_grouping_placement("FROM clause of its own query level");
        let from = DataSource::from_ast_list_in_scope(ast.from(), context, with, &from_scope)?;
        if let Some(scope) = outer {
            scope.validate_rule_sources(context, &from)?;
        }
        Ok(from)
    }
}

impl SelectStatement {
    pub(crate) fn ast_scope<'a>(
        ast: &Select,
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&ScalarAnalysisContext<'a>>,
        query_id: usize,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::nested(with, from, outer);
        scope.set_grouping_query(query_id);
        scope.set_windows(QueryWindow::ast_names(ast.windows()));
        scope
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_windows(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        QueryWindow::from_ast_definitions(ast.windows(), context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_where(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(
            ast.predicate(),
            context,
            &scope.with_grouping_placement("WHERE"),
        )
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_targets(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in ast.targets() {
            ResultColumnCollection::ast_append_to_results(
                target,
                context,
                with,
                from,
                scope,
                &mut results,
            )?;
        }
        Ok(results)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_distinct(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &ResultColumnCollection,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        let distinct = if let Some(distinct) = &ast.distinct() {
            let mut resolved = Vec::new();
            for expression in distinct {
                resolved.push(SemScalarExpr::from_ast_clause(
                    expression,
                    context,
                    scope,
                    results,
                    ClauseBinding::Order,
                )?);
            }
            Some(resolved)
        } else {
            None
        };
        Ok(distinct)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_grouping(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &mut ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        query_id: usize,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(results);
        }
        let group = QueryGrouping::from_ast_list(ast.group(), context, scope, results)?;
        for grouping in &group {
            grouping.validate_grouping_targets(results, query_id, context)?;
        }
        context
            .analysis_state_mut()
            .grouping_mut()
            .set_keys(query_id, &group, results)?;
        Ok(group)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_having(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(ast.having(), context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_clauses(
        ast: &Select,
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Result<SelectClauses, AnalysisError> {
        let mut clauses =
            SelectClauses::new(distinct, group, ast.group_distinct(), having, windows);
        let destination = ast
            .into()
            .as_ref()
            .map(SelectDestination::from_ast_destination)
            .transpose()?;
        clauses.set_destination(destination);
        Ok(clauses)
    }
}

impl SelectStatement {
    pub(crate) fn ast_build_result(
        results: ResultColumnCollection,
        from: FromClause,
        predicate: Option<SemScalarExpr>,
        clauses: SelectClauses,
    ) -> Self {
        Self::from(SelectStatementNode::from_resolution(
            results,
            &WithClause::default(),
            from,
            predicate,
            clauses,
        ))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        query: &Query,
    ) -> Result<CteStatement, AnalysisError> {
        let mut query = Self::from_ast_in(query, context, Some(scope))?;
        query.rename_columns(columns);
        Ok(CteStatement::from_query(query))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set(
        ast: &SetQuery,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let left = Self::from_ast_in(ast.left(), context, Some(&scope))?;
        let right = Self::from_ast_in(ast.right(), context, Some(&scope))?;
        Self::from_ast_set_operation(&ast.operation(), context, left, right)
    }
}

impl SelectStatement {
    pub(crate) const fn ast_is_compound(ast: &QueryBody) -> bool {
        matches!(ast, QueryBody::Set(_))
    }
}

impl SelectStatement {
    pub(crate) fn ast_window_names(ast: &QueryBody) -> Vec<String> {
        match ast {
            QueryBody::Select(select) => QueryWindow::ast_names(select.windows()),
            _ => Vec::new(),
        }
    }
}

impl SelectStatement {
    pub(crate) fn ast_set_all(ast: &QueryBody) -> bool {
        match ast {
            QueryBody::Set(set) => set.all(),
            _ => false,
        }
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &Query,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        for entry in ast.with().entries() {
            CteStatement::validate_ast_select_rule_ctes(entry.body(), identity)?;
        }
        Ok(())
    }
}
