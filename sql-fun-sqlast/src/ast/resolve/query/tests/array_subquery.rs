//! ARRAY query semantics through the lightweight query-analysis boundary.
mod test_helpers;

use super::super::test_helpers::resolve;
use crate::ast::Dialect;
use crate::sem::{ParseContext, PgBuiltInType, SelectStatement, SemScalarExpr, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::one("SELECT 1")]
#[case::empty("SELECT 1 WHERE false")]
#[case::many("SELECT 1 UNION ALL SELECT 2")]
#[case::null("SELECT NULL::integer")]
#[case::unknown_null("SELECT NULL")]
#[case::unknown_string("SELECT 'hello'")]
#[case::array("SELECT ARRAY[1,2]")]
#[case::empty_array_result("SELECT NULL::integer[] WHERE false")]
#[case::nested("SELECT ARRAY(SELECT 1)")]
#[case::star("SELECT s.* FROM (SELECT 1 AS n) s")]
#[case::clauses("SELECT 1 AS n ORDER BY n LIMIT 2 OFFSET 0")]
fn array_query_retains_child_type_domain_identity(
    #[case] child: &str,
    #[values(false, true)] infer: bool,
) {
    let (query, context) = resolve(
        &format!("SELECT ARRAY({child}) AS result"),
        Dialect::postgres(),
        infer,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let columns = query.result_columns();
    let column = columns.get_column_at(0).unwrap();
    let array = column.expression().unwrap().as_array_subquery().unwrap();
    assert_eq!(array.query().result_columns().len(), 1);
    assert_eq!(
        array.types().single_type().as_ref(),
        column.col_type().as_ref()
    );
}

#[rstest::rstest]
#[case::zero("SELECT")]
#[case::two("SELECT 1,2")]
#[case::star("SELECT s.* FROM (SELECT 1 AS n, 2 AS m) s")]
fn invalid_output_shape_retains_query_and_empty_domain(
    #[case] child: &str,
    #[values(false, true)] infer: bool,
) {
    let (query, _) = resolve(
        &format!("SELECT ARRAY({child})"),
        Dialect::postgres(),
        infer,
    )
    .unwrap();
    let array = projection(&query).as_array_subquery().unwrap();
    assert_ne!(array.query().result_columns().len(), 1);
    assert_eq!(array.types().admissible_types(), Some(&[][..]));
}

#[rstest::rstest]
#[case::projection("SELECT ARRAY(SELECT $1::integer)")]
#[case::predicate("SELECT ARRAY(SELECT 1 WHERE $1::integer IS NULL)")]
#[case::order("SELECT ARRAY(SELECT 1 ORDER BY $1::integer)")]
#[case::correlated("SELECT ARRAY(SELECT s.n WHERE $1::integer IS NULL) FROM (SELECT 1 AS n) s")]
#[case::nested("SELECT ARRAY(SELECT ARRAY(SELECT $1::integer))")]
#[case::repeated("SELECT ARRAY(SELECT $1::integer), $1::integer")]
fn child_parameters_are_visited_and_finalized(#[case] sql: &str) {
    let (mut query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    let mut parameters = Parameters(Vec::new());
    query.walk(&mut parameters).unwrap();
    assert!(!parameters.0.is_empty());
    assert!(parameters.0.into_iter().all(|ty| ty.is_some()));
}

#[test]
fn array_query_projection_preserves_both_source_shapes_and_correlations() {
    let (query, context) = resolve(
        "SELECT CASE WHEN true THEN ARRAY(SELECT $1) ELSE $2::integer[] END",
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
    let constraints = context.parameter_constraints();
    for input in [PgBuiltInType::int4(), PgBuiltInType::int4().array_type()] {
        assert!(constraints.allows(&BTreeMap::from([(1, input)])));
    }
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    assert_eq!(
        projection(&query).get_type(),
        Some(PgBuiltInType::int4().array_type())
    );
}

#[test]
fn unconstrained_source_keeps_functional_array_relationship() {
    let (query, context) = resolve("SELECT ARRAY(SELECT $1)", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let constraints = context.parameter_constraints();
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
    assert!(
        projection(&query)
            .as_array_subquery()
            .unwrap()
            .types()
            .is_any()
    );
}

#[test]
fn anonymous_record_identity_is_retained_in_array_type() {
    let (query, context) =
        resolve("SELECT ARRAY(SELECT ROW(1,'x'))", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let array = projection(&query).get_type().unwrap();
    assert_eq!(array.is_array(), Some(true));
    assert!(array.element_type().type_definition().is_some());
}

#[test]
fn correlated_binding_and_serialized_query_survive_analysis() {
    let (query, context) = resolve(
        "SELECT ARRAY(SELECT s.n WHERE s.n IS NOT NULL ORDER BY s.n) FROM (SELECT 1 AS n) s",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let array = projection(&query).as_array_subquery().unwrap();
    let child = projection(array.query());
    assert!(child.constraint_binding().is_some());
    assert_eq!(child.get_type(), Some(PgBuiltInType::int4()));
    let encoded = ::serde_json::to_string(projection(&query)).unwrap();
    let decoded: SemScalarExpr = ::serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.get_type(), projection(&query).get_type());
    assert!(decoded.as_array_subquery().is_some());
}

fn projection(query: &SelectStatement) -> &SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}

struct Parameters(Vec<Option<TypeReference>>);
impl crate::sem::traversal::SemanticVisitor for Parameters {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), crate::sem::AnalysisError> {
        if matches!(value, SemScalarExpr::Param(_)) {
            self.0.push(value.get_type());
        }
        Ok(())
    }
}

#[test]
fn synthetic_identity_function_consumes_array_subquery_result() {
    let (query, context) = resolve(
        "SELECT identity((ARRAY(SELECT 1))[1])",
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
    assert_eq!(projection(&query).get_type(), Some(PgBuiltInType::int4()));
}

#[test]
fn invalid_array_query_does_not_stop_following_statement() {
    let mut context = super::super::test_helpers::catalog();
    let mut results = Vec::new();
    for statement in crate::parse_statements(
        "SELECT ARRAY(SELECT 1,2); SELECT ARRAY(SELECT 1)",
        Dialect::postgres(),
    ) {
        let analyzed = statement.unwrap().analyze(context).unwrap();
        results.push(analyzed.ast().clone().into_select_statemtnt().unwrap());
        context = analyzed.into_context();
    }
    assert!(reports_width_problem(context.reported_problems()));
    assert_eq!(projection(&results[0]).get_type(), None);
    assert_eq!(
        projection(&results[1]).get_type(),
        Some(PgBuiltInType::int4().array_type())
    );
}

#[rstest::rstest]
#[case::named("SELECT ARRAY(SELECT NULL::singleton)", "singleton")]
#[case::named_pair("SELECT ARRAY(SELECT NULL::pair)", "pair")]
fn named_composite_elements_keep_nominal_identity(#[case] sql: &str, #[case] name: &str) {
    let analyzed = test_helpers::composite_query(sql);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expected =
        TypeReference::concrete_type_ref(&crate::sem::FullName::with_schema("public", name), false)
            .array_type();
    assert_eq!(projection(&query).get_type(), Some(expected));
}

#[rstest::rstest]
#[case::one("singleton", true)]
#[case::two("pair", false)]
fn deferred_star_checks_final_output_width(#[case] name: &str, #[case] valid: bool) {
    let analyzed = test_helpers::composite_query(&format!(
        "SELECT ARRAY(SELECT ($1).*) AS result, take_{name}($1)"
    ));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let array = projection(&query).as_array_subquery().unwrap();
    if valid {
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        assert_eq!(
            projection(&query).get_type(),
            Some(PgBuiltInType::int4().array_type())
        );
    } else {
        assert!(reports_width_problem(analyzed.context().problems()));
        assert_eq!(array.types().admissible_types(), Some(&[][..]));
    }
    assert_eq!(
        array.query().result_columns().len(),
        if valid { 1 } else { 2 }
    );
}

#[test]
fn deferred_star_preserves_enclosing_result_link() {
    let analyzed =
        test_helpers::composite_query("SELECT (ARRAY(SELECT ($1).*))[1], take_singleton($1)");
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(projection(&query).get_type(), Some(PgBuiltInType::int4()));
}

fn reports_width_problem(problems: &[crate::sem::AnalysisProblem]) -> bool {
    for problem in problems {
        if problem.code() == "array_subquery_columns" {
            return true;
        }
    }
    false
}

#[test]
fn nested_deferred_arrays_finish_after_each_projection_expands() {
    let analyzed = test_helpers::composite_query(
        "SELECT ARRAY(SELECT ((ARRAY(SELECT ($1).*))[1]).*), take_container($1)",
    );
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        projection(&query).get_type(),
        Some(PgBuiltInType::int4().array_type())
    );
}
