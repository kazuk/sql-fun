mod test_helpers;

#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
use crate::{
    sem::{CastContext, CastDefinition, ScalarConstExpr, scalar_expr::distinct::DistinctExpr},
    test_helpers::TestParseContext,
};

use self::test_helpers::{context, expression, query, schema};
use crate::sem::{ParseContext, PgBuiltInType, SemScalarExpr};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::distinct("DISTINCT", false)]
#[case::not_distinct("NOT DISTINCT", true)]
fn retains_equality_binding_and_polarity(
    #[case] kind: &str,
    #[case] negated: bool,
    #[values("1", "$1")] operand: &str,
) -> testresult::TestResult {
    let (query, _) = query(&format!("SELECT {operand} IS {kind} FROM 1"), context())?;
    let expr = expression(&query);
    let distinct = expr.as_distinct().unwrap();
    assert_eq!(distinct.negated(), negated);
    assert!(
        distinct
            .equality()
            .unwrap()
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    let encoded = ::serde_json::to_string(expr)?;
    let decoded: SemScalarExpr = ::serde_json::from_str(&encoded)?;
    assert_eq!(decoded.as_distinct().unwrap().negated(), negated);
    assert!(
        decoded
            .as_distinct()
            .unwrap()
            .equality()
            .unwrap()
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    Ok(())
}

#[rstest::rstest]
#[case::left("SELECT NULL IS DISTINCT FROM $1")]
#[case::right("SELECT $1 IS NOT DISTINCT FROM NULL")]
fn bare_null_does_not_impose_equality_or_type(#[case] sql: &str) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    let test = expression(&query).as_distinct().unwrap();
    assert!(test.equality().is_none());
    assert_eq!(test.comparisons().len(), 2);
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    Ok(())
}

#[rstest::rstest]
#[case::simple("SELECT $1 IS DISTINCT FROM 1")]
#[case::repeated("SELECT $1 IS NOT DISTINCT FROM 1, $1 IS DISTINCT FROM 2")]
#[case::nested("SELECT ($1 IS DISTINCT FROM 1) IS NOT DISTINCT FROM false")]
#[case::row("SELECT ROW($1, NULL) IS DISTINCT FROM ROW(1, NULL)")]
#[case::case_result("SELECT CASE WHEN true THEN $1 IS DISTINCT FROM 1 ELSE false END")]
#[case::derived("SELECT flag FROM (SELECT $1 IS DISTINCT FROM 1 AS flag) c")]
fn constrains_comparison_inputs_without_replacing_predicate(
    #[case] sql: &str,
) -> testresult::TestResult {
    let (_, context) = query(sql, context())?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert_eq!(
        constraints.result_column(0).unwrap().single_type(),
        Some(PgBuiltInType::bool())
    );
    Ok(())
}

#[test]
fn parameter_assignments_are_correlated() -> testresult::TestResult {
    let (_, context) = query("SELECT $1 IS DISTINCT FROM $2", context())?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::int4())
    ])));
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::bool()),
        (2, PgBuiltInType::bool())
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::bool())
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::arity("SELECT ROW(1,2) IS DISTINCT FROM ROW(1)", "distinct_row_arity")]
#[case::incompatible(
    "SELECT 1 IS DISTINCT FROM true",
    "binary_operator_overload_resolution_failed"
)]
fn retains_invalid_comparisons(#[case] sql: &str, #[case] code: &str) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    assert!(
        context
            .problems()
            .iter()
            .any(|problem| problem.code() == code)
    );
    assert!(expression(&query).as_distinct().is_some());
    Ok(())
}

#[test]
fn binds_columns_and_retains_null_literal_row_distinction() -> testresult::TestResult {
    let context = schema("CREATE TABLE flags(nullable boolean, required boolean NOT NULL)")?;
    let (query, _) = query(
        "SELECT nullable IS DISTINCT FROM required, ROW(NULL,NULL) IS DISTINCT FROM NULL FROM flags",
        context,
    )?;
    let test = expression(&query).as_distinct().unwrap();
    assert!(
        test.equality()
            .unwrap()
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    let test = query
        .result_columns()
        .get_column_at(1)
        .unwrap()
        .expression()
        .unwrap()
        .as_distinct()
        .unwrap();
    assert!(test.comparisons()[0].is_row_constructor());
    assert!(test.equality().is_none());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::distinct("1 IS DISTINCT FROM NULL", false, 1, false)]
#[case::not_distinct("NULL IS NOT DISTINCT FROM NULL", true, 1, false)]
#[case::is("1 IS 1", true, 1, false)]
#[case::is_not("1 IS NOT 1", false, 0, false)]
#[case::mixed("'1' IS DISTINCT FROM 1", false, 1, false)]
#[case::truth("2 IS TRUE", true, 1, false)]
#[case::truth_distinct("2 IS NOT DISTINCT FROM TRUE", true, 1, false)]
#[case::collation("'A' COLLATE NOCASE IS NOT DISTINCT FROM 'a'", true, 1, true)]
fn sqlite_predicates_preserve_operands_without_catalog_equality(
    #[case] sql: &str,
    #[case] negated: bool,
    #[case] oracle: i32,
    #[case] missing_collation: bool,
) -> testresult::TestResult {
    let sql = format!("SELECT {sql}");
    let connection = ::rusqlite::Connection::open_in_memory()?;
    assert_eq!(
        connection.query_row(&sql, [], |row| row.get::<_, i32>(0))?,
        oracle
    );
    let context = test_helpers::context_for(Dialect::sqlite());
    let (query, context) = query(&sql, context)?;
    let test = expression(&query).as_distinct().unwrap();
    assert_eq!(test.negated(), negated);
    assert!(test.equality().is_none());
    assert_eq!(test.comparisons().len(), 2);
    if missing_collation {
        assert_eq!(context.problems().len(), 1);
        assert_eq!(context.problems()[0].code(), "collation_not_found");
        assert!(
            test.comparisons()[0]
                .as_collate()
                .unwrap()
                .collation()
                .is_missing()
        );
    } else {
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_comparisons_do_not_constrain_input_affinity() -> testresult::TestResult {
    let context = test_helpers::context_for(Dialect::sqlite());
    let (_, context) = query("SELECT ?1 IS DISTINCT FROM 1, ?1 IS NOT NULL", context)?;
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::text(),
        PgBuiltInType::bool(),
    ] {
        assert!(
            context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, ty)]))
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::non_strict(false)]
#[case::strict(true)]
fn custom_equality_strictness_does_not_guarantee_non_null(
    #[case] strict: bool,
) -> testresult::TestResult {
    let mut context = TestParseContext::default();
    context.setup_type(PgBuiltInType::int4());
    context.setup_binary("=", "int4", "int4", "bool", strict);
    let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
    let predicate = SemScalarExpr::distinct(DistinctExpr::resolve(
        &mut context,
        value.clone(),
        value,
        false,
    )?);
    assert_eq!(predicate.is_not_null(), None);
    assert_eq!(predicate.get_type(), Some(PgBuiltInType::bool()));
    Ok(())
}

#[test]
fn non_boolean_equality_is_rejected_and_retained() -> testresult::TestResult {
    let mut context = TestParseContext::default();
    context.setup_type(PgBuiltInType::int4());
    context.setup_binary("=", "int4", "int4", "int4", true);
    let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
    let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
    assert!(context.has_reported_problem("distinct_equality_result"));
    assert_eq!(predicate.types().admissible_types(), Some([].as_slice()));
    assert_eq!(
        predicate.equality().unwrap().get_type(),
        Some(PgBuiltInType::int4())
    );
    Ok(())
}

#[test]
fn retains_implicit_equality_coercions() -> testresult::TestResult {
    let mut context = TestParseContext::default();
    context.setup_type(PgBuiltInType::int4());
    context.setup_type(PgBuiltInType::int8());
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.setup_binary("=", "int8", "int8", "bool", true);
    let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
    let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
    assert_eq!(
        predicate.left().unwrap().get_type(),
        Some(PgBuiltInType::int8())
    );
    assert_eq!(
        predicate.right().unwrap().get_type(),
        Some(PgBuiltInType::int8())
    );
    Ok(())
}

#[test]
fn missing_equality_retains_failed_binding() -> testresult::TestResult {
    let mut context = TestParseContext::default();
    let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
    let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
    assert!(context.has_reported_problem("undefined_operator"));
    assert!(
        !predicate
            .equality()
            .unwrap()
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    assert_eq!(predicate.types().admissible_types(), Some([].as_slice()));
    assert!(predicate.left().is_some());
    Ok(())
}

#[test]
fn conflicting_comparisons_have_no_admissible_assignment() -> testresult::TestResult {
    let (_, context) = query(
        "SELECT $1 IS DISTINCT FROM 1, $1 IS NOT DISTINCT FROM false",
        context(),
    )?;
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::bool(),
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
