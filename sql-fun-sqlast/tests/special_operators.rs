//! Special comparison inference against the shipped PostgreSQL overload/cast catalog.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{ParseContext, PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::array_any("SELECT $1 = ANY($2)")]
#[case::array_all("SELECT $1 <> ALL($2)")]
#[case::array_constructor("SELECT $1 < ALL(ARRAY[$2, 3])")]
#[case::typed_array("SELECT $1 >= ANY(ARRAY[1,2])")]
#[case::typed_left("SELECT 1 <= ALL($1)")]
#[case::nullif("SELECT NULLIF($1,1)")]
#[case::between("SELECT $1 BETWEEN 1 AND 2")]
#[case::not_between("SELECT $1 NOT BETWEEN 1 AND 2")]
#[case::query_any("SELECT $1 = ANY(SELECT 1)")]
#[case::query_all("SELECT $1 < ALL(SELECT 2)")]
#[case::nested("SELECT NULLIF($1,1) = ANY(SELECT $2 + 1)")]
#[case::cte("WITH c AS (SELECT $1 = ANY($2) AS b) SELECT b FROM c")]
fn real_catalog_comparisons_retain_bindings_and_domains(
    #[case] sql: &str,
) -> testresult::TestResult {
    // Builtin overload competition and cast-source families are the assertion's contract.
    let analyzed = heavy::analyze(sql, false)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{sql}: {:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.parameter(1).is_some());
    let first = if sql == "SELECT 1 <= ALL($1)" {
        PgBuiltInType::int4().array_type()
    } else {
        PgBuiltInType::int4()
    };
    assert!(constraints.allows(&BTreeMap::from([(1, first)])));
    Ok(())
}

#[rstest::rstest]
#[case::promoted("SELECT NULLIF(1,2.2)", PgBuiltInType::numeric())]
#[case::unknown_left("SELECT NULLIF('1',2)", PgBuiltInType::int4())]
#[case::null_left("SELECT NULLIF(NULL,2)", PgBuiltInType::int4())]
#[case::both_null("SELECT NULLIF(NULL,NULL)", PgBuiltInType::text())]
fn nullif_exposes_coerced_left_result(
    #[case] sql: &str,
    #[case] expected: TypeReference,
) -> testresult::TestResult {
    // These assertions require PostgreSQL's actual numeric and unknown-literal overload selection.
    let analyzed = heavy::analyze(sql, false)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(expected.clone()));
    assert_eq!(column.is_not_null(), &Some(false));
    let comparison = &column
        .expression()
        .unwrap()
        .as_special_operator()
        .unwrap()
        .comparisons()[0];
    assert!(comparison.targets()[0].allows(&expected));
    Ok(())
}

#[rstest::rstest]
#[case::array_columns("SELECT ARRAY[1] = ANY(SELECT ARRAY[1])")]
#[case::null_query_left("SELECT NULL = ANY(SELECT 1)")]
#[case::empty_query("SELECT 1 < ALL(SELECT 2 WHERE false)")]
#[case::scalar_control("SELECT (SELECT 1), EXISTS(SELECT 1), ARRAY(SELECT 1)")]
fn query_forms_preserve_static_types(#[case] sql: &str) -> testresult::TestResult {
    // Actual array-polymorphic operators distinguish array-valued query columns from array elements.
    let analyzed = heavy::analyze(sql, false)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        select
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::known_scalar("SELECT $1 = ANY(1)", "require_array")]
#[case::width("SELECT $1 = ANY(SELECT 1,2)", "comparison_subquery_columns")]
#[case::non_boolean("SELECT $1 + ANY($2)", "parameter_constraint")]
fn invalid_operands_have_explicit_failures(
    #[case] sql: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, false)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy;
}
