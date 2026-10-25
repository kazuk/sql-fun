//! Range families resolved against the shipped PostgreSQL catalog.
mod test_helpers {
    pub(crate) mod heavy;
}

use ::sql_fun_sqlast::sem::TypeReference;

#[rstest::rstest]
#[case::range_bound("lower(NULL::int4range)", "int4")]
#[case::multirange_bound("lower(NULL::int4multirange)", "int4")]
#[case::range_merge("range_merge(NULL::int4range,NULL::int4range)", "int4range")]
#[case::multirange_merge("range_merge(NULL::int4multirange)", "int4range")]
#[case::range_to_multi("multirange(NULL::int4range)", "int4multirange")]
#[case::range_union("NULL::int4range + NULL::int4range", "int4range")]
#[case::range_equal("NULL::int4range = NULL::int4range", "bool")]
#[case::multi_union("NULL::int4multirange + NULL::int4multirange", "int4multirange")]
fn catalog_range_overloads(
    #[case] expression: &str,
    #[case] result: &str,
) -> testresult::TestResult {
    // Actual pg_range facts and builtin signatures are the contract under test.
    let analyzed = test_helpers::heavy::analyze(&format!("SELECT {expression}"), false)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(TypeReference::from_master_type_name(result))
    );
    Ok(())
}
