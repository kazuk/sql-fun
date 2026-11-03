use crate::ast::wildcard::rows_source::freeze;
use crate::test_helpers::wildcard_rows_source::invocation;
use crate::test_helpers::wildcard_rows_source::layout;
use crate::test_helpers::wildcard_rows_source::rendered;

#[rstest::rstest]
#[case::plain(false)]
#[case::ordinality(true)]
fn zips_fixed_function_projections_without_exposing_helper_columns(#[case] ordinality: bool) {
    let suffix = if ordinality { " WITH ORDINALITY" } else { "" };
    let source = invocation(&format!(
        "SELECT * FROM ROWS FROM (generate_series(1,2),generate_series(10,13)){suffix} AS r"
    ));
    let outputs: &[&str] = if ordinality {
        &["id", "id", "position"]
    } else {
        &["id", "id"]
    };
    let resolved = layout(
        &[&["generate_series"], &["generate_series"]],
        outputs,
        ordinality,
    );
    let frozen = freeze(&resolved, source).unwrap();
    let sql = rendered(frozen);
    assert!(sql.ends_with(r#") AS "r""#), "{sql}");
    ::pg_query::parse(&sql, ::pg_query::ParserOptions::default()).unwrap();
    assert_eq!(
        sql.matches(r#""generate_series"(1, 2)"#).count(),
        1,
        "{sql}"
    );
    assert_eq!(
        sql.matches(r#""generate_series"(10, 13)"#).count(),
        1,
        "{sql}"
    );
    assert_eq!(sql.matches(r#""row_number"()"#).count(), 2, "{sql}");
    assert!(sql.contains("FULL JOIN"), "{sql}");
    assert_eq!(sql.matches("AS MATERIALIZED").count(), 2, "{sql}");
    assert_eq!(sql.matches(r#""count"(*)"#).count(), 1, "{sql}");
    assert!(sql.contains(r#"OPERATOR("pg_catalog".>=)"#), "{sql}");
    assert!(sql.contains("ORDER BY"), "{sql}");
}

#[test]
fn retains_per_function_record_definitions() {
    let source = invocation(
        r#"SELECT * FROM ROWS FROM (json_to_recordset('[{"id":1}]') AS (id int4),generate_series(1,2)) AS r"#,
    );
    let resolved = layout(&[&["id"], &["generate_series"]], &["id", "id"], false);
    let sql = rendered(freeze(&resolved, source).unwrap());
    ::pg_query::parse(&sql, ::pg_query::ParserOptions::default()).unwrap();
    assert!(sql.contains(r#"AS ("id" "int4")"#), "{sql}");
}

#[rstest::rstest]
#[case::missing_function(&[])]
#[case::missing_output(&[&["generate_series"][..]][..])]
fn mismatched_layout_rejects_before_building_a_source(#[case] functions: &[&[&str]]) {
    let source = invocation("SELECT * FROM ROWS FROM (generate_series(1,2)) AS r");
    let resolved = layout(functions, &[], false);
    assert!(
        freeze(&resolved, source)
            .unwrap_err()
            .is_invalid_expression()
    );
}
