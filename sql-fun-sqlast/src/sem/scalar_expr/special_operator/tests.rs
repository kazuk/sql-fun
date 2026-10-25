mod test_helpers;
use crate::sem::{ParseContext, PgBuiltInType};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::any("SELECT $1 = ANY($2)")]
#[case::all("SELECT $1 < ALL($2)")]
fn array_inputs_keep_correlated_element_relationships(#[case] sql: &str) {
    let analyzed = test_helpers::analyze(sql);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::int4().array_type())
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::text().array_type())
    ])));
    assert!(!constraints.allows(&BTreeMap::from([(2, PgBuiltInType::int4())])));
    assert_eq!(
        constraints.result_column(0).unwrap().admissible_types(),
        Some([PgBuiltInType::bool()].as_slice())
    );
}

#[rstest::rstest]
#[case::between("SELECT $1 BETWEEN 1 AND 2")]
#[case::nested("SELECT NULLIF($1, 1) BETWEEN 1 AND 2")]
#[case::not_between("SELECT $1 NOT BETWEEN 1 AND 2")]
#[case::any_subquery("SELECT $1 = ANY(SELECT 1)")]
#[case::all_subquery("SELECT $1 <> ALL(SELECT 1)")]
fn comparisons_constrain_outer_parameters(#[case] sql: &str) {
    let analyzed = test_helpers::analyze(sql);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = select
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_special_operator()
        .unwrap();
    assert!(!expression.comparisons().is_empty());
    assert!(
        expression.comparisons()[0]
            .binding()
            .resolution()
            .is_resolved()
    );
}

#[test]
fn nullif_returns_promoted_left_comparison_type() {
    let analyzed = test_helpers::analyze("SELECT NULLIF(1, 2::numeric)");
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::numeric()));
    assert_eq!(column.is_not_null(), &Some(false));
}

#[test]
fn subquery_width_failure_retains_outer_and_inner_expressions() {
    let analyzed = test_helpers::analyze("SELECT ($1, 2) = ANY(SELECT 1)");
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "comparison_subquery_columns")
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = select
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_special_operator()
        .unwrap();
    assert_eq!(expression.operands().len(), 2);
    assert_eq!(expression.query().unwrap().result_columns().len(), 1);
    assert_eq!(expression.types().admissible_types(), Some([].as_slice()));
}

#[rstest::rstest]
#[case::between(
    "SELECT 10 BETWEEN 1 AND 100",
    crate::sem::SpecialOperatorKind::between()
)]
#[case::not_between(
    "SELECT 10 NOT BETWEEN 1 AND 100",
    crate::sem::SpecialOperatorKind::not_between()
)]
fn typed_between_retains_both_comparisons(
    #[case] sql: &str,
    #[case] kind: crate::sem::SpecialOperatorKind,
) {
    let analyzed = test_helpers::analyze(sql);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::bool()));
    let expression = column.expression().unwrap().as_special_operator().unwrap();
    assert_eq!(expression.kind(), kind);
    assert_eq!(expression.operands().len(), 3);
    assert_eq!(expression.comparisons().len(), 2);
}

#[rstest::rstest]
#[case::nullif("SELECT NULLIF($1, $2)")]
#[case::query("SELECT $1 = ANY(SELECT $2)")]
#[case::correlation("SELECT $1 = ANY(SELECT $2 WHERE $1 > 0)")]
fn both_sides_retain_input_positions(#[case] sql: &str) {
    let analyzed = test_helpers::analyze(sql);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.parameter(1).is_some());
    assert!(constraints.parameter(2).is_some());
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::int4())
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::text()),
        (2, PgBuiltInType::int4())
    ])));
}

#[rstest::rstest]
#[case::any("SELECT $1 = ANY($2), $2::integer")]
#[case::between("SELECT $1 BETWEEN 1 AND 2, $1::boolean")]
#[case::nullif("SELECT NULLIF($1,1), $1::boolean")]
fn conflicting_uses_do_not_become_unrestricted(#[case] sql: &str) {
    let analyzed = test_helpers::analyze(sql);
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "parameter_constraint"),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
}
