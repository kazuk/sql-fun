//! Public truth-test analysis with the shipped `PostgreSQL` cast catalog.
mod test_helpers {
    mod cache_context;
    pub(crate) use self::cache_context::create_cache_context;

    pub(super) mod heavy;
}

use self::test_helpers::heavy;
use ::sql_fun_sqlast::context::ParseContext;
use ::sql_fun_sqlast::sem::PgBuiltInType;
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::is_true("TRUE", Some(true), false)]
#[case::is_not_true("NOT TRUE", Some(true), true)]
#[case::is_false("FALSE", Some(false), false)]
#[case::is_not_false("NOT FALSE", Some(false), true)]
#[case::is_unknown("UNKNOWN", None, false)]
#[case::is_not_unknown("NOT UNKNOWN", None, true)]
fn shipped_catalog_preserves_semantics_and_constraints(
    #[case] kind: &str,
    #[case] value: Option<bool>,
    #[case] negated: bool,
) -> testresult::TestResult {
    // Actual builtin casts must reject integer/text inputs while accepting bool.
    let analyzed = heavy::analyze(&format!("SELECT $1 IS {kind}, NULL IS {kind}"))?;
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for index in 0..2 {
        let column = query.result_columns().get_column_at(index).unwrap();
        let test = column.expression().unwrap().as_truth_test().unwrap();
        assert_eq!(test.value(), value);
        assert_eq!(test.negated(), negated);
        assert_eq!(test.argument().get_type(), Some(PgBuiltInType::bool()));
    }
    Ok(())
}

#[rstest::rstest]
#[case::integer("1 IS TRUE")]
#[case::typed_text("'true'::text IS NOT FALSE")]
#[case::invalid_literal("'invalid' IS UNKNOWN")]
fn shipped_catalog_retains_rejected_operand(#[case] expression: &str) -> testresult::TestResult {
    // Verify the real catalog does not confuse explicit casts with implicit ones.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"))?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let test = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_truth_test()
        .unwrap();
    assert_eq!(test.types().admissible_types(), Some([].as_slice()));
    assert!(test.argument().get_type().is_some());
    Ok(())
}
