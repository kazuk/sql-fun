//! Public semantic outcomes, using the lightweight query catalog.

use crate::sem::convert;

use crate::sem::SemAst;
use crate::test_helpers::grouping::groupings;

use crate::ast::Dialect;
use crate::ast::StatementNode;
use crate::context::ParseContext;
use crate::parse_statements;
use crate::sem::GroupingExpr;
use crate::sem::PgBuiltInType;
use crate::sem::SemScalarExpr;
use crate::test_helpers::create_cache_context as test_helpers_create_cache_context;
use crate::test_helpers::query::catalog;
use crate::test_helpers::query::grouping_table_context;
use crate::test_helpers::query::resolve;

#[rstest::rstest]
#[case::ordinary("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::rollup("SELECT GROUPING(b,a,b) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY ROLLUP(a,b)", &[1,0,1])]
#[case::cube("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY CUBE(a,b)", &[1,0])]
#[case::sets("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY GROUPING SETS ((a,b),(),(a))", &[1,0])]
#[case::duplicate_keys("SELECT GROUPING(a,a) FROM (SELECT 1 AS a) s GROUP BY a,a", &[0,0])]
#[case::later_parameter("SELECT GROUPING(a+1), $2 FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
#[case::qualified_function("SELECT GROUPING(identity(a)) FROM (SELECT 1 AS a) s GROUP BY public.identity(a)", &[0])]
#[case::repeated_before_another_key("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY a,a,b", &[2,0])]
#[case::expression("SELECT GROUPING(s.a+1) FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
#[case::alias("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY x", &[0])]
#[case::ordinal("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY 1", &[0])]
#[case::input_precedence("SELECT a+1 AS a, GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::coalesce("SELECT COALESCE(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::nested_local_grouping("SELECT sum((SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::group_alias_local_subquery("SELECT (SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a) AS g GROUP BY g", &[0])]
#[case::constant("SELECT GROUPING(1+1) GROUP BY (1+1)", &[0])]
#[case::parentheses("SELECT GROUPING((a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::noop_argument_cast("SELECT GROUPING(CAST(a AS integer)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
#[case::noop_key_cast("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY CAST(a AS integer)", &[0])]
#[case::having("SELECT a FROM (SELECT 1 AS a) s GROUP BY a HAVING GROUPING(a) IS NOT NULL", &[0])]
#[case::order("SELECT a FROM (SELECT 1 AS a) s GROUP BY a ORDER BY GROUPING(a)", &[0])]
#[case::window_order("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a WINDOW w AS (ORDER BY GROUPING(a))", &[0])]
#[case::window_aggregate("SELECT sum(GROUPING(a)) OVER () FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
fn binds_grouping_keys(
    #[case] sql: &str,
    #[case] indices: &[usize],
    #[values(false, true)] infer: bool,
) {
    let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{sql}: {:?}",
        context.reported_problems()
    );
    let groupings = groupings(&query);
    assert!(!groupings.is_empty());
    for grouping in groupings {
        assert!(grouping.is_valid(), "{sql}: {grouping:?}");
        assert_eq!(grouping.levels_up(), 0);
        assert_eq!(grouping.arguments().len(), indices.len());
        for (argument, index) in grouping.arguments().iter().zip(indices) {
            assert_eq!(argument.key_index(), Some(*index));
        }
    }
}

#[rstest::rstest]
#[case::mismatch(
    "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a+1",
    "query_grouping_argument_not_grouped"
)]
#[case::function_missing(
    "SELECT GROUPING(missing(a)) FROM (SELECT 1 AS a) s GROUP BY missing(a)",
    "query_grouping_argument_not_grouped"
)]
#[case::both_missing(
    "SELECT GROUPING(missing) GROUP BY missing",
    "query_grouping_argument_not_grouped"
)]
#[case::missing(
    "SELECT GROUPING(missing) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_grouping_argument_not_grouped"
)]
#[case::ambiguous(
    "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
    "query_grouping_argument_not_grouped"
)]
#[case::where_clause(
    "SELECT a FROM (SELECT 1 AS a) s WHERE GROUPING(a) IS NULL GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::group_clause(
    "SELECT 1 FROM (SELECT 1 AS a) s GROUP BY GROUPING(a)",
    "query_invalid_grouping_placement"
)]
#[case::group_alias(
    "SELECT GROUPING(a) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
    "query_invalid_grouping_placement"
)]
#[case::group_ordinal(
    "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY 1,a",
    "query_invalid_grouping_placement"
)]
#[case::group_wrapped_alias(
    "SELECT GROUPING(a)+1 AS g FROM (SELECT 1 AS a) s GROUP BY ROLLUP(g,a)",
    "query_invalid_grouping_placement"
)]
#[case::group_correlated_alias(
    "SELECT (SELECT GROUPING(s.a)) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
    "query_invalid_grouping_placement"
)]
#[case::nested(
    "SELECT GROUPING(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::aggregate_argument(
    "SELECT sum(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::correlated_aggregate_inside(
    "SELECT GROUPING((SELECT sum(s.a))) FROM (SELECT 1 AS a) s GROUP BY (SELECT sum(s.a))",
    "query_invalid_grouping_placement"
)]
#[case::aggregate_inside(
    "SELECT GROUPING(sum(a)) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::filter(
    "SELECT sum(a) FILTER (WHERE GROUPING(a) IS NULL) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::frame(
    "SELECT sum(a) OVER (ROWS GROUPING(a) PRECEDING) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::row_limit("SELECT 1 LIMIT GROUPING(1)", "query_invalid_grouping_placement")]
#[case::no_grouping("SELECT GROUPING(1)", "query_grouping_argument_not_grouped")]
#[case::inner_constant(
    "SELECT (SELECT GROUPING(1)) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_grouping_argument_not_grouped"
)]
#[case::correlated_where(
    "SELECT 1 FROM (SELECT 1 AS a) s WHERE (SELECT GROUPING(s.a)) IS NULL GROUP BY a",
    "query_invalid_grouping_placement"
)]
#[case::correlated_aggregate(
    "SELECT sum((SELECT GROUPING(s.a))) FROM (SELECT 1 AS a) s GROUP BY a",
    "query_invalid_grouping_placement"
)]
fn reports_grouping_errors(#[case] sql: &str, #[case] code: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{sql}: {:?}",
        context.reported_problems()
    );
    let groupings = groupings(&query);
    assert!(!groupings.is_empty());
    for grouping in groupings {
        assert!(!grouping.is_valid(), "{sql}: {grouping:?}");
    }
}

#[rstest::rstest]
#[case::outer("SELECT (SELECT GROUPING(s.a)) FROM (SELECT 1 AS a) s GROUP BY a", 1)]
#[case::shadowed(
    "SELECT (SELECT GROUPING(s.a) FROM (SELECT 2 AS a) s GROUP BY s.a) FROM (SELECT 1 AS a) s GROUP BY a",
    0
)]
#[case::mixed(
    "SELECT (SELECT GROUPING(s.a+t.a) FROM (SELECT 2 AS a) t GROUP BY s.a+t.a) FROM (SELECT 1 AS a) s GROUP BY a",
    0
)]
fn binds_the_owning_query(#[case] sql: &str, #[case] levels_up: usize) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    for grouping in groupings(&query) {
        assert!(grouping.is_valid(), "{grouping:?}");
        assert_eq!(grouping.levels_up(), levels_up);
        assert_eq!(grouping.arguments()[0].key_index(), Some(0));
    }
}

#[rstest::rstest]
#[case::correlated_from(
    "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x",
    false,
    1
)]
#[case::correlated_join(
    "SELECT * FROM (VALUES (1)) s(a) JOIN LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x ON true",
    false,
    1
)]
#[case::local(
    "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a) FROM (VALUES(2)) t(a) GROUP BY t.a) x",
    true,
    0
)]
#[case::mixed(
    "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a+s.a) FROM (VALUES(2)) t(a) GROUP BY t.a+s.a) x",
    true,
    0
)]
fn binds_grouping_in_lateral_sources(
    #[case] sql: &str,
    #[case] valid: bool,
    #[case] levels_up: usize,
    #[values(false, true)] infer: bool,
) {
    let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
    let groupings = groupings(&query);
    assert!(!groupings.is_empty());
    for grouping in groupings {
        assert_eq!(grouping.is_valid(), valid, "{sql}: {grouping:?}");
        assert_eq!(grouping.levels_up(), levels_up, "{sql}: {grouping:?}");
        assert_eq!(
            grouping.arguments()[0].key_index(),
            if valid { Some(0) } else { None }
        );
    }
    if valid {
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{sql}: {:?}",
            context.reported_problems()
        );
    } else {
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "query_invalid_grouping_placement"),
            "{sql}: {:?}",
            context.reported_problems()
        );
    }
}

#[test]
fn retains_type_nullability_and_parameter_operands() {
    let (query, context) = resolve(
        "SELECT GROUPING(a+$7) FROM (SELECT 1 AS a) s GROUP BY a+$7",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
    assert_eq!(expression.is_not_null(), Some(true));
    assert_eq!(expression.get_column_name(), "grouping");
    let parameter = context.parameter_constraints();
    let constraint = parameter.parameter(7).unwrap().constraint();
    assert!(constraint.allows(&PgBuiltInType::int4()));
    assert!(!constraint.allows(&PgBuiltInType::text()));
    let grouping = expression.as_grouping().unwrap();
    assert!(grouping.is_valid());
    assert_eq!(
        grouping.arguments()[0].expression().get_type(),
        Some(PgBuiltInType::int4())
    );
}

#[rstest::rstest]
#[case::maximum(31, true)]
#[case::too_many(32, false)]
fn enforces_argument_limit(#[case] count: usize, #[case] valid: bool) {
    let arguments = vec!["a"; count].join(",");
    let (query, context) = resolve(
        &format!("SELECT GROUPING({arguments}) FROM (SELECT 1 AS a) s GROUP BY a"),
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), usize::from(!valid));
    let groupings = groupings(&query);
    assert_eq!(groupings[0].arguments().len(), count);
    assert_eq!(groupings[0].is_valid(), valid);
}

#[test]
fn rejects_invalid_grouping_through_diagnostic_policy() {
    let statement = parse_statements("SELECT GROUPING(1)", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.node else {
        panic!("query");
    };
    let mut context = catalog();
    context.reject_diagnostics_after(0);
    assert!(
        convert(&query)
            .resolve(
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_err()
    );
    assert_eq!(context.reported_problem_count(), 0);
}

#[test]
fn grouping_result_type_does_not_constrain_argument_type() {
    let (query, context) = resolve(
        "SELECT GROUPING(CAST($1 AS text)) GROUP BY CAST($1 AS text)",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let grouping = groupings(&query).pop().unwrap();
    assert!(grouping.is_valid());
    assert_eq!(
        grouping.arguments()[0].expression().get_type(),
        Some(PgBuiltInType::text())
    );
    let serialized = ::serde_json::to_string(&grouping).unwrap();
    let restored: GroupingExpr = ::serde_json::from_str(&serialized).unwrap();
    assert!(restored.is_valid());
    assert_eq!(restored.arguments()[0].key_index(), Some(0));
}

#[rstest::rstest]
#[case::insert_value("INSERT INTO t VALUES(GROUPING(1))")]
#[case::insert_returning("INSERT INTO t VALUES(1) RETURNING GROUPING(a)")]
#[case::update_value("UPDATE t SET a=GROUPING(a)")]
#[case::update_returning("UPDATE t SET a=1 RETURNING GROUPING(a)")]
#[case::delete_where("DELETE FROM t WHERE GROUPING(a) IS NULL")]
#[case::delete_returning("DELETE FROM t RETURNING GROUPING(a)")]
fn rejects_grouping_outside_select(#[case] sql: &str) {
    let mut cache_context = test_helpers_create_cache_context();
    let context = grouping_table_context();
    let analysis = SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap(),
        context,
        &mut cache_context,
    )
    .unwrap();
    assert!(
        analysis
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "query_invalid_grouping_placement")
    );
}

#[rstest::rstest]
#[case::missing("SELECT GROUPING(missing) GROUP BY missing", false)]
#[case::ambiguous(
    "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
    true
)]
fn preserves_failed_argument_lookup(#[case] sql: &str, #[case] ambiguous: bool) {
    let (query, _) = resolve(sql, Dialect::postgres(), false).unwrap();
    let grouping = groupings(&query).pop().unwrap();
    assert!(!grouping.is_valid());
    assert_eq!(grouping.arguments()[0].key_index(), None);
    let expression = grouping.arguments()[0].expression();
    if ambiguous {
        ::assert_matches::assert_matches!(
            expression,
            SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_))
        );
    } else {
        ::assert_matches::assert_matches!(
            expression,
            SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(_))
        );
    }
}
