//! Domain coercions retain nominal results through the public analysis boundary.
// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use crate::test_helpers::heavy::domain_casts as heavy;
use ::sql_fun_sqlast::sem::TypeReference;

#[rstest::rstest]
#[case::identity(
    "1::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::smallint(
    "1::smallint::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::bigint(
    "1::bigint::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::string_io(
    "1::bigint::information_schema.character_data",
    "information_schema.character_data"
)]
#[case::identifier(
    "1::smallint::information_schema.sql_identifier",
    "information_schema.sql_identifier"
)]
#[case::timestamp(
    "NULL::timestamptz::information_schema.time_stamp",
    "information_schema.time_stamp"
)]
#[case::null(
    "NULL::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::source_domain("1::public.amount::bigint", "pg_catalog.int8")]
#[case::same_domain("1::public.amount::public.amount", "public.amount")]
#[case::nested("1::public.nested_amount", "public.nested_amount")]
#[case::empty_array("ARRAY[]::public.amounts", "public.amounts")]
#[case::array_domain("ARRAY[1]::public.amounts", "public.amounts")]
#[case::domain_elements("ARRAY[1]::public.amount[]", "public.amount[]")]
fn explicit_cast_retains_domain_binding(
    #[values("17", "18")] version: &str,
    #[case] expression: &str,
    #[case] target: &str,
) -> testresult::TestResult {
    // Real builtin conversions, including I/O casts, are the contract here.
    let analyzed = heavy::analyze(version, &format!("SELECT {expression}"))?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected: TypeReference = target.parse()?;
    let encoded = ::serde_json::to_value(column.expression().unwrap())?;
    assert_eq!(
        encoded["TypeCast"]["target_type"],
        ::serde_json::to_value(expected)?
    );
    assert!(!encoded["TypeCast"]["arg"].is_null());
    assert_eq!(
        encoded["TypeCast"]["resolution"]["Type"],
        encoded["TypeCast"]["target_type"]
    );
    Ok(())
}

#[rstest::rstest]
#[case::invalid("true::information_schema.time_stamp", "explicit_cast_not_defined")]
#[case::invalid_array(
    "NULL::boolean[]::information_schema.time_stamp[]",
    "explicit_cast_not_defined"
)]
#[case::missing("1::public.no_such_domain", "parameter_constraint")]
fn invalid_cast_keeps_target_lookup_and_argument(
    #[values("17", "18")] version: &str,
    #[case] expression: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(version, &format!("SELECT {expression}"))?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let encoded = ::serde_json::to_value(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap(),
    )?;
    assert!(!encoded["TypeCast"]["arg"].is_null());
    if expression.contains("no_such_domain") {
        assert_eq!(encoded["TypeCast"]["resolution"], "NotFound");
    } else {
        assert_eq!(
            encoded["TypeCast"]["resolution"]["Type"],
            encoded["TypeCast"]["target_type"]
        );
    }
    Ok(())
}

// Removed the remaining PostgreSQL 17 parameter_cast_preserves_admissible_inputs_and_domain_output
// timeout case; restore from 1906f3271e57fea65bef60e47d993fc265529370.
// PostgreSQL 18 was previously removed in fc49a6a15367c4b1e7c02c3f920bb3f7338ee0d6.

// Removed shipped_postgres18_catalog_domain_casts_resolve; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
