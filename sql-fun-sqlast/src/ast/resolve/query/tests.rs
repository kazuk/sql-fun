//! Semantic behavior at the query-analysis boundary. All fixtures use a small catalog.
mod call_arguments;
mod column_scope;
mod exists;

use super::test_helpers::{output_index, output_type, query, resolve};
use crate::ast::Dialect;
use crate::sem::{
    ColumnReferenceExpr, ParseContext, PgBuiltInType, QueryGrouping, QueryNameResolution,
    SemScalarExpr,
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
    #[case] ty: crate::sem::TypeReference,
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
#[case::order("SELECT n AS x FROM (SELECT 1 AS n) s ORDER BY x", Some(0))]
#[case::ordinal("SELECT 1 AS x ORDER BY 1", Some(0))]
#[case::input_precedence("SELECT 1 AS n FROM (SELECT true AS n) s GROUP BY n", None)]
fn binds_aliases_and_ordinals(#[case] sql: &str, #[case] expected: Option<usize>) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let query = query.query_clauses().unwrap();
    let expression = if let Some(order) = query.order().first() {
        order.expression()
    } else {
        let QueryGrouping::Expression(expression) =
            &query.body().select_clauses().unwrap().group()[0]
        else {
            panic!("grouping expression");
        };
        expression
    };
    assert_eq!(output_index(expression), expected);
}

#[rstest::rstest]
#[case::missing_order("SELECT 1 ORDER BY missing", false)]
#[case::ambiguous_order("SELECT 1 AS x, 2 AS x ORDER BY x", true)]
#[case::having_has_no_alias("SELECT 1 AS x HAVING x IS NULL", false)]
fn preserves_failed_column_resolution(#[case] sql: &str, #[case] ambiguous: bool) {
    let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert!(context.reported_problem_count() > 0);
    struct Failures {
        missing: usize,
        ambiguous: usize,
    }
    impl crate::sem::traversal::SemanticVisitor for Failures {
        fn expression(
            &mut self,
            expression: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            match expression {
                SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_)) => {
                    self.missing += 1
                }
                SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_)) => {
                    self.ambiguous += 1
                }
                _ => {}
            }
            Ok(())
        }
    }
    let mut failures = Failures {
        missing: 0,
        ambiguous: 0,
    };
    query.walk(&mut failures).unwrap();
    if ambiguous {
        assert_eq!(failures.ambiguous, 1);
    } else {
        assert_eq!(failures.missing, 1);
    }
}

#[rstest::rstest]
#[case::found(
    "SELECT sum(1) OVER w WINDOW w AS ()",
    QueryNameResolution::WindowAlias(0),
    0
)]
#[case::missing("SELECT sum(1) OVER w", QueryNameResolution::NotFound, 1)]
#[case::ambiguous(
    "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
    QueryNameResolution::Ambiguous,
    2
)]
fn records_window_resolution(
    #[case] sql: &str,
    #[case] expected: QueryNameResolution,
    #[case] problems: usize,
) {
    let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), problems);
    struct Windows(Vec<QueryNameResolution>);
    impl crate::sem::traversal::SemanticVisitor for Windows {
        fn expression(
            &mut self,
            expression: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if let SemScalarExpr::FuncCall(call) = expression {
                self.0.push(
                    call.modifiers()
                        .over()
                        .unwrap()
                        .reference()
                        .unwrap()
                        .resolution()
                        .clone(),
                );
                assert_matches::assert_matches!(
                    call.name_resolution(),
                    QueryNameResolution::Function(_)
                );
            }
            Ok(())
        }
    }
    let mut windows = Windows(Vec::new());
    query.walk(&mut windows).unwrap();
    assert_eq!(windows.0, [expected]);
}

#[test]
fn compound_boundary_owns_its_trailing_clauses() {
    let query =
        query("(SELECT 1 AS n LIMIT 1) UNION ALL (SELECT 2 LIMIT 2) ORDER BY n LIMIT 3 OFFSET 4");
    let boundary = query.query_clauses().unwrap();
    assert!(boundary.set_all());
    assert_eq!(boundary.order().len(), 1);
    assert!(boundary.limit().is_some());
    assert!(boundary.offset().is_some());
    let crate::sem::SelectStatement::Union(left, right) = boundary.body() else {
        panic!("compound body");
    };
    assert!(left.query_clauses().unwrap().order().is_empty());
    assert!(right.query_clauses().unwrap().order().is_empty());
}

#[test]
fn ctes_are_owned_once_in_declaration_order() {
    let (mut query, context) = resolve(
        "WITH a AS (SELECT $1 + 1 AS n), b AS (SELECT n FROM a) SELECT n FROM b ORDER BY n",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let [first, second] = query.query_clauses().unwrap().with() else {
        panic!("two local CTE declarations");
    };
    assert_eq!(first.name(), "a");
    assert_eq!(second.name(), "b");
    struct Parameters(usize);
    impl crate::sem::traversal::SemanticVisitor for Parameters {
        fn expression(
            &mut self,
            expression: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if matches!(expression, SemScalarExpr::Param(_)) {
                self.0 += 1;
            }
            Ok(())
        }
    }
    let mut parameters = Parameters(0);
    query.walk(&mut parameters).unwrap();
    assert_eq!(parameters.0, 1);
    assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::int4()));
}

#[rstest::rstest]
#[case::clauses("SELECT 1 AS n GROUP BY n HAVING true ORDER BY n LIMIT 1 OFFSET 0")]
#[case::nested("WITH a AS (SELECT 1 AS n ORDER BY n) SELECT n FROM a ORDER BY (SELECT 1)")]
#[case::compound("SELECT 1 UNION ALL SELECT 2 ORDER BY 1 LIMIT 1")]
fn parameter_free_queries_skip_inference(#[case] sql: &str) {
    let before = crate::sem::parameter_resolution::inference_runs();
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

#[rstest::rstest]
#[case::inherited_numeric(
    "SELECT sum(1) OVER (w RANGE $7 PRECEDING) WINDOW w AS (ORDER BY 1)",
    PgBuiltInType::int4()
)]
#[case::range_date(
    "SELECT sum(1) OVER (ORDER BY $8::date RANGE $7 PRECEDING)",
    crate::sem::TypeReference::from_master_type_name("interval")
)]
#[case::using("SELECT 1 ORDER BY $7 USING <", PgBuiltInType::int4())]
fn uses_clause_specific_constraints(#[case] sql: &str, #[case] ty: crate::sem::TypeReference) {
    let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let domain = context
        .parameter_constraints()
        .parameter(7)
        .unwrap()
        .constraint();
    assert!(!domain.is_any());
    assert!(domain.allows(&ty));
    assert!(!domain.allows(&PgBuiltInType::text()));
}

#[test]
fn retains_function_modifiers_after_inference_without_duplicate_operands() {
    let (mut query, context) = resolve(
        "SELECT sum($1 + 1 ORDER BY $2 + 1) FILTER (WHERE $3) OVER w WINDOW w AS ()",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    struct Calls {
        functions: usize,
        parameters: Vec<u32>,
    }
    impl crate::sem::traversal::SemanticVisitor for Calls {
        fn expression(
            &mut self,
            expression: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if let SemScalarExpr::Param(parameter) = expression {
                self.parameters.push(parameter.position());
            }
            if let SemScalarExpr::Constrained(value) = expression
                && let Some(binding) = value.function_binding()
            {
                self.functions += 1;
                assert_matches::assert_matches!(
                    binding.resolution(),
                    QueryNameResolution::Function(_)
                );
                assert_matches::assert_matches!(
                    binding
                        .modifiers()
                        .over()
                        .unwrap()
                        .reference()
                        .unwrap()
                        .resolution(),
                    QueryNameResolution::WindowAlias(0)
                );
                assert_eq!(
                    binding.modifiers().filter().unwrap().get_type(),
                    Some(PgBuiltInType::bool())
                );
            }
            Ok(())
        }
    }
    let mut calls = Calls {
        functions: 0,
        parameters: Vec::new(),
    };
    query.walk(&mut calls).unwrap();
    calls.parameters.sort();
    assert_eq!(calls.parameters, [1, 2, 3]);
    assert_eq!(calls.functions, 1);
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

#[test]
fn missing_window_inheritance_is_an_explicit_failed_lookup() {
    let (query, context) = resolve(
        "SELECT sum(1) OVER w WINDOW w AS (missing)",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    let window = &query
        .query_clauses()
        .unwrap()
        .body()
        .select_clauses()
        .unwrap()
        .windows()[0];
    assert_matches::assert_matches!(
        window.reference().unwrap().resolution(),
        QueryNameResolution::NotFound
    );
}

#[test]
fn parameter_conflicts_across_clauses_produce_diagnostics() {
    let (_, context) = resolve("SELECT 1 HAVING $1 LIMIT $1", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problems().len(), 1);
    assert_eq!(
        context.reported_problems()[0].code(),
        "parameter_constraint"
    );
}

#[test]
fn values_refresh_result_metadata_after_inference() {
    let (query, context) = resolve("VALUES ($1 + 1)", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    assert_eq!(output_type(&query, "column1"), Some(PgBuiltInType::int4()));
}

#[test]
fn inference_uses_captured_function_and_operator_bindings() {
    let (mut query, mut catalog) = resolve(
        "SELECT sum($1 + 1) FILTER (WHERE $2) ORDER BY $3 + 1",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    catalog.setup_function("sum", &[]);
    catalog.set_get_operator_op_result(
        &crate::sem::OperatorName::from_parts(None, "+"),
        true,
        true,
        None,
    );
    catalog.begin_parameter_type_analysis();
    let constraints = crate::sem::parameter_resolution::ParameterResolution::infer_query(
        &mut query,
        &mut catalog,
    )
    .unwrap();
    catalog.finish_analysis(Ok(())).unwrap();
    assert_eq!(catalog.reported_problem_count(), 0);
    assert!(
        constraints
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int4())
    );
    assert!(!constraints.parameter(1).unwrap().constraint().is_any());
    assert_eq!(
        constraints
            .parameter(2)
            .unwrap()
            .constraint()
            .admissible_types(),
        Some([PgBuiltInType::bool()].as_slice())
    );
    assert!(!constraints.parameter(3).unwrap().constraint().is_any());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::right_alias("SELECT 1 AS n UNION ALL SELECT 2 AS m ORDER BY m")]
#[case::right_expression(
    "SELECT CAST(1 AS TEXT) UNION ALL SELECT CAST(2 AS TEXT) ORDER BY CAST(2 AS TEXT)"
)]
#[case::duplicate_alias("SELECT 1 AS n, 2 AS n ORDER BY n")]
fn sqlite_binds_ordering_to_its_output(#[case] sql: &str) {
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    connection.prepare(sql).unwrap();
    let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        output_index(query.query_clauses().unwrap().order()[0].expression()),
        Some(0)
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::forward(
    "SELECT 1 WINDOW w AS (later), later AS ()",
    QueryNameResolution::WindowAlias(1)
)]
#[case::missing_base("SELECT 1 WINDOW w AS (missing)", QueryNameResolution::NotFound)]
fn sqlite_window_inheritance_records_dialect_binding(
    #[case] sql: &str,
    #[case] expected: QueryNameResolution,
) {
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    connection.prepare(sql).unwrap();
    let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let clauses = query
        .query_clauses()
        .unwrap()
        .body()
        .select_clauses()
        .unwrap();
    assert_eq!(
        clauses.windows()[0].reference().unwrap().resolution(),
        &expected
    );
}

#[rstest::rstest]
#[case::missing_operator("SELECT 1 ORDER BY 1 USING #", false)]
#[case::missing_lock("SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent", true)]
fn retains_failed_clause_name_bindings(#[case] sql: &str, #[case] lock: bool) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    let boundary = query.query_clauses().unwrap();
    let outcome = if lock {
        boundary.locks()[0].relations()[0].resolution()
    } else {
        boundary.order()[0].using().unwrap().resolution()
    };
    assert_matches::assert_matches!(outcome, QueryNameResolution::NotFound);
}

#[test]
fn failed_function_name_lookup_survives_inference() {
    let (mut query, context) = resolve("SELECT absent($1)", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    struct Bindings(usize);
    impl crate::sem::traversal::SemanticVisitor for Bindings {
        fn expression(
            &mut self,
            expression: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if let SemScalarExpr::Constrained(value) = expression
                && let Some(binding) = value.function_binding()
            {
                assert_matches::assert_matches!(
                    binding.resolution(),
                    QueryNameResolution::NotFound
                );
                self.0 += 1;
            }
            Ok(())
        }
    }
    let mut bindings = Bindings(0);
    query.walk(&mut bindings).unwrap();
    assert_eq!(bindings.0, 1);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_window_definitions_do_not_bind_projection_aliases() {
    let (query, context) = resolve(
        "SELECT 1 AS n WINDOW w AS (ORDER BY n)",
        Dialect::sqlite(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    let windows = query
        .query_clauses()
        .unwrap()
        .body()
        .select_clauses()
        .unwrap()
        .windows();
    assert_matches::assert_matches!(
        windows[0].order()[0].expression(),
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_))
    );
}

#[rstest::rstest]
#[case::duplicate_window(
    "SELECT 1 WINDOW w AS (), w AS ()",
    vec![crate::sem::QueryProblem::duplicate_window_definition("w")]
)]
#[case::missing_window(
    "SELECT sum(1) OVER absent",
    vec![crate::sem::QueryProblem::reference_not_found("absent")]
)]
#[case::ambiguous_window(
    "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
    vec![crate::sem::QueryProblem::duplicate_window_definition("w"), crate::sem::QueryProblem::ambiguous_reference("w")]
)]
#[case::missing_operator(
    "SELECT 1 ORDER BY 1 USING #",
    vec![crate::sem::QueryProblem::reference_not_found("#")]
)]
#[case::missing_lock_source(
    "SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent",
    vec![crate::sem::QueryProblem::reference_not_found("absent")]
)]
#[case::zero_position(
    "SELECT 1 ORDER BY 0",
    vec![crate::sem::QueryProblem::position_out_of_range(0)]
)]
#[case::group_position(
    "SELECT 1 GROUP BY 2",
    vec![crate::sem::QueryProblem::position_out_of_range(2)]
)]
#[case::compound_expression(
    "SELECT 1 UNION SELECT 2 ORDER BY 1 + 1",
    vec![crate::sem::QueryProblem::invalid_compound_order()]
)]
fn reports_structured_query_problems(
    #[case] sql: &str,
    #[case] expected: Vec<crate::sem::QueryProblem>,
) {
    let (_, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    let mut actual = Vec::new();
    for problem in context.reported_problems() {
        if let Some(problem) = problem.query_problem() {
            actual.push(problem.clone());
        }
    }
    assert_eq!(actual, expected);
}
