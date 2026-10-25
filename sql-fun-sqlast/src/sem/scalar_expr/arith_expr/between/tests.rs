mod test_helpers;

use self::test_helpers::{context, expression, query, schema};
use crate::sem::{AnalysisError, ParseContext, PgBuiltInType, SemScalarExpr};
use std::collections::BTreeMap;

#[derive(Default)]
struct Bindings(Vec<String>);

impl crate::sem::traversal::SemanticVisitor for Bindings {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(binding) = expression.operator_binding() {
            assert!(binding.resolution().is_resolved());
            let encoded = ::serde_json::to_value(binding).unwrap();
            self.0.push(
                encoded["name"]["Valid"]["op_name"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[rstest::rstest]
#[case::positive("", ">=", "<=")]
#[case::negative("NOT ", "<", ">")]
fn retains_comparison_bindings_and_serialized_operands(
    #[case] negation: &str,
    #[case] lower: &str,
    #[case] upper: &str,
) -> testresult::TestResult {
    let (query, context) = query(
        &format!("SELECT 2 {negation}BETWEEN SYMMETRIC 3 AND 1"),
        context(&[lower, upper]),
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let original = expression(&query);
    let mut decoded: SemScalarExpr = ::serde_json::from_str(&::serde_json::to_string(original)?)?;
    assert_eq!(&decoded, original);
    assert_eq!(decoded.get_type(), Some(PgBuiltInType::bool()));
    let mut bindings = Bindings::default();
    decoded.walk(&mut bindings)?;
    bindings.0.sort();
    let mut expected = vec![lower, lower, upper, upper];
    expected.sort();
    assert_eq!(bindings.0, expected);
    Ok(())
}

#[rstest::rstest]
#[case::first("$1 BETWEEN SYMMETRIC 3 AND 1")]
#[case::lower("2 BETWEEN SYMMETRIC $1 AND 1")]
#[case::upper("2 NOT BETWEEN SYMMETRIC 3 AND $1")]
#[case::nested("COALESCE($1, 2) BETWEEN SYMMETRIC 3 AND 1")]
fn constrains_all_parameter_positions(#[case] predicate: &str) -> testresult::TestResult {
    let (_, context) = query(
        &format!("SELECT {predicate}"),
        context(&[">=", "<=", "<", ">"]),
    )?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    Ok(())
}

#[rstest::rstest]
#[case::positive("")]
#[case::negative("NOT ")]
fn preserves_correlated_parameter_assignments(#[case] negation: &str) -> testresult::TestResult {
    let (_, context) = query(
        &format!("SELECT $1 {negation}BETWEEN SYMMETRIC $2 AND $3"),
        context(&[">=", "<=", "<", ">"]),
    )?;
    let constraints = context.parameter_constraints();
    for ty in [PgBuiltInType::int4(), PgBuiltInType::text()] {
        assert!(constraints.allows(&BTreeMap::from([(1, ty.clone()), (2, ty.clone()), (3, ty)])));
    }
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::text()),
        (3, PgBuiltInType::int4()),
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::text()),
        (2, PgBuiltInType::text()),
        (3, PgBuiltInType::int4()),
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::first("SELECT $1 BETWEEN SYMMETRIC 3 AND 1, $1::boolean")]
#[case::last("SELECT $1::boolean, $1 NOT BETWEEN SYMMETRIC 3 AND 1")]
fn conflicting_uses_reject_assignments(#[case] sql: &str) -> testresult::TestResult {
    let (_, context) = query(sql, context(&[">=", "<=", "<", ">"]))?;
    for ty in [
        PgBuiltInType::bool(),
        PgBuiltInType::int4(),
        PgBuiltInType::text(),
    ] {
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, ty)]))
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::resolved("SELECT v.value BETWEEN SYMMETRIC 3 AND 1 FROM vals v", "TableColumn")]
#[case::missing(
    "SELECT missing BETWEEN SYMMETRIC 3 AND 1 FROM vals",
    "UndefinedColumn"
)]
#[case::ambiguous(
    "SELECT value NOT BETWEEN SYMMETRIC 3 AND 1 FROM vals a, vals b",
    "AmbiguousColumnRef"
)]
fn retains_column_lookup_outcomes(
    #[case] sql: &str,
    #[case] outcome: &str,
) -> testresult::TestResult {
    let (query, _) = query(sql, schema("CREATE TABLE vals(value int4)")?)?;
    let encoded = ::serde_json::to_string(expression(&query))?;
    assert!(encoded.contains(outcome), "{encoded}");
    Ok(())
}

#[rstest::rstest]
#[case::positive("")]
#[case::negative("NOT ")]
fn repeated_parameter_can_use_each_comparable_type(
    #[case] negation: &str,
) -> testresult::TestResult {
    let (_, context) = query(
        &format!("SELECT $1 {negation}BETWEEN SYMMETRIC $1 AND $1"),
        context(&[">=", "<=", "<", ">"]),
    )?;
    for ty in [PgBuiltInType::int4(), PgBuiltInType::text()] {
        assert!(
            context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, ty)]))
        );
    }
    assert!(
        !context
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

#[rstest::rstest]
#[case::positive("", &[">=", "<="])]
#[case::negative("NOT ", &["<", ">"])]
fn bounds_need_no_comparison_with_each_other(
    #[case] negation: &str,
    #[case] operators: &[&str],
) -> testresult::TestResult {
    let (query, context) = query(
        &format!("SELECT 2 {negation}BETWEEN SYMMETRIC true AND 'end'::text"),
        test_helpers::mixed_bounds_context(operators),
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::bool()));
    Ok(())
}
