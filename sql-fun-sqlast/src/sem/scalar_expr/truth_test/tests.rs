mod test_helpers;

use crate::sem::SemScalarExpr;

use self::test_helpers::{context, expression, query, schema};
use crate::sem::{CastContext, ParseContext, PgBuiltInType};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::is_true("TRUE", Some(true), false)]
#[case::is_not_true("NOT TRUE", Some(true), true)]
#[case::is_false("FALSE", Some(false), false)]
#[case::is_not_false("NOT FALSE", Some(false), true)]
#[case::is_unknown("UNKNOWN", None, false)]
#[case::is_not_unknown("NOT UNKNOWN", None, true)]
fn retains_all_six_kinds_and_operand_coercions(
    #[case] kind: &str,
    #[case] value: Option<bool>,
    #[case] negated: bool,
    #[values("TRUE", "FALSE", "NULL", "NULL::boolean", "$1")] operand: &str,
) -> testresult::TestResult {
    let (query, _) = query(&format!("SELECT {operand} IS {kind}"), context(None))?;
    let expr = expression(&query);
    let test = expr.as_truth_test().unwrap();
    assert_eq!(test.value(), value);
    assert_eq!(test.negated(), negated);
    assert_eq!(test.argument().get_type(), Some(PgBuiltInType::bool()));
    let encoded = ::serde_json::to_string(expr)?;
    let decoded: SemScalarExpr = ::serde_json::from_str(&encoded)?;
    assert_eq!(decoded.as_truth_test().unwrap().value(), value);
    assert_eq!(decoded.as_truth_test().unwrap().negated(), negated);
    Ok(())
}

#[rstest::rstest]
#[case::integer("1 IS TRUE")]
#[case::typed_text("'true'::text IS TRUE")]
#[case::invalid("'invalid' IS FALSE")]
#[case::ambiguous("'o' IS TRUE")]
#[case::empty("'' IS UNKNOWN")]
#[case::invalid_nested("(1 IS TRUE) IS FALSE")]
fn retains_invalid_operands_and_empty_result_domains(
    #[case] input: &str,
    #[values("", ", $1")] parameter: &str,
) -> testresult::TestResult {
    let (query, context) = query(&format!("SELECT {input}{parameter}"), context(None))?;
    let test = expression(&query).as_truth_test().unwrap();
    assert_eq!(test.types().admissible_types(), Some([].as_slice()));
    if !parameter.is_empty() {
        assert_eq!(
            context
                .parameter_constraints()
                .result_column(0)
                .unwrap()
                .admissible_types(),
            Some([].as_slice())
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::identity(None, false)]
#[case::implicit(Some(CastContext::implicit()), true)]
#[case::assignment(Some(CastContext::assignment()), false)]
#[case::explicit(Some(CastContext::explicit()), false)]
fn accepts_only_implicit_boolean_conversions(
    #[case] cast: Option<CastContext>,
    #[case] accepted: bool,
) -> testresult::TestResult {
    let (query, _) = query("SELECT 'true'::text IS TRUE", context(cast))?;
    let test = expression(&query).as_truth_test().unwrap();
    assert_eq!(test.types().single_type().is_some(), accepted);
    if accepted {
        assert_eq!(test.argument().get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_value(test.argument())?;
        assert!(encoded.get("ImplicitCast").is_some());
    }
    Ok(())
}

#[rstest::rstest]
#[case::plain("SELECT $1 IS TRUE")]
#[case::negated("SELECT $1 IS NOT UNKNOWN")]
#[case::repeated("SELECT $1 IS FALSE, $1 IS NOT TRUE")]
#[case::cast_first("SELECT $1::boolean, $1 IS TRUE")]
#[case::cast_last("SELECT $1 IS TRUE, $1::boolean")]
#[case::nested_test("SELECT ($1 IS TRUE) IS FALSE")]
#[case::case_operand("SELECT (CASE WHEN true THEN $1 ELSE false END) IS TRUE")]
#[case::coalesce_operand("SELECT COALESCE($1,false) IS TRUE")]
#[case::case_result("SELECT CASE WHEN true THEN $1 IS TRUE ELSE false END")]
#[case::case_condition("SELECT CASE WHEN $1 IS TRUE THEN true ELSE false END")]
#[case::coalesce_result("SELECT COALESCE($1 IS TRUE,false)")]
#[case::cte("WITH c AS (SELECT $1 IS TRUE AS flag) SELECT flag FROM c")]
#[case::derived("SELECT flag FROM (SELECT $1 IS TRUE AS flag) c")]
fn propagates_boolean_parameter_requirements(#[case] sql: &str) -> testresult::TestResult {
    let (_, context) = query(sql, context(None))?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    Ok(())
}

#[rstest::rstest]
#[case::first("SELECT $1 IS TRUE, $1 = 1")]
#[case::last("SELECT $1 = 1, $1 IS TRUE")]
fn conflicting_parameter_uses_have_no_admissible_assignment(
    #[case] sql: &str,
) -> testresult::TestResult {
    let (_, context) = query(sql, context(None))?;
    assert!(
        !context
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

#[rstest::rstest]
#[case::nullable("nullable", Some(false))]
#[case::not_null("required", Some(true))]
fn resolves_columns_and_preserves_operand_nullability(
    #[case] column: &str,
    #[case] nullable: Option<bool>,
    #[values("TRUE", "NOT TRUE", "FALSE", "NOT FALSE", "UNKNOWN", "NOT UNKNOWN")] kind: &str,
) -> testresult::TestResult {
    let context = schema("CREATE TABLE flags(nullable boolean, required boolean NOT NULL)")?;
    let (query, _) = query(&format!("SELECT {column} IS {kind} FROM flags"), context)?;
    let expr = expression(&query);
    let argument = expr.as_truth_test().unwrap().argument();
    assert_eq!(argument.is_not_null(), nullable);
    assert_eq!(
        argument.get_column_def().unwrap().get_type_cloned(),
        Some(PgBuiltInType::bool())
    );
    Ok(())
}

#[test]
fn accepts_boolean_domain_operands_and_parameters() -> testresult::TestResult {
    let context = schema("CREATE DOMAIN flag AS boolean; CREATE TABLE flags(value flag)")?;
    let (query, context) = query("SELECT value IS UNKNOWN, $1 IS TRUE FROM flags", context)?;
    assert_eq!(
        expression(&query)
            .as_truth_test()
            .unwrap()
            .argument()
            .get_type(),
        Some(PgBuiltInType::bool())
    );
    assert!(
        context
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, test_helpers::bool_domain())]))
    );
    Ok(())
}

#[test]
fn continues_after_invalid_truth_test() -> testresult::TestResult {
    let (_, context) = query("SELECT 1 IS TRUE", context(None))?;
    let (query, _) = query("SELECT false IS FALSE", context)?;
    assert!(expression(&query).as_truth_test().is_some());
    Ok(())
}

#[rstest::rstest]
#[case::missing("SELECT missing IS TRUE FROM flags", "UndefinedColumn")]
#[case::ambiguous("SELECT value IS TRUE FROM flags a, flags b", "AmbiguousColumnRef")]
fn retains_failed_column_bindings(
    #[case] sql: &str,
    #[case] binding: &str,
) -> testresult::TestResult {
    let context = schema("CREATE TABLE flags(value boolean)")?;
    let (query, _) = query(sql, context)?;
    let encoded = ::serde_json::to_value(expression(&query).as_truth_test().unwrap().argument())?;
    // A boolean coercion must retain the completed failed lookup underneath it.
    assert!(
        encoded["ImplicitCast"]["value"]["ColumnRef"]
            .get(binding)
            .is_some(),
        "{encoded}"
    );
    Ok(())
}

#[test]
fn implicit_source_parameters_keep_their_source_domains() -> testresult::TestResult {
    let (query, context) = query("SELECT $1 IS TRUE", context(Some(CastContext::implicit())))?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let encoded = ::serde_json::to_value(expression(&query).as_truth_test().unwrap().argument())?;
    assert!(
        encoded["ImplicitCast"]["value"].get("Param").is_some(),
        "{encoded}"
    );
    Ok(())
}

#[rstest::rstest]
#[case::coalesce("SELECT COALESCE($1,$2) IS TRUE")]
#[case::case("SELECT (CASE WHEN true THEN $1 ELSE $2 END) IS NOT UNKNOWN")]
fn joint_parameters_are_constrained_through_operand(#[case] sql: &str) -> testresult::TestResult {
    let (query, context) = query(sql, context(None))?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::bool()),
        (2, PgBuiltInType::bool()),
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::bool()),
        (2, PgBuiltInType::int4()),
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::text()),
        (2, PgBuiltInType::text()),
    ])));
    assert_eq!(
        expression(&query)
            .as_truth_test()
            .unwrap()
            .argument()
            .get_type(),
        Some(PgBuiltInType::bool())
    );
    Ok(())
}
