use crate::ast::Dialect;
use crate::test_helpers::data_source_table::analyze;
use crate::test_helpers::data_source_table::assert_problem;
use crate::{
    names::FullName,
    sem::{ColumnReferenceExpr, ParseContext, SemScalarExpr, TypeReference},
};

#[rstest::rstest]
#[case::tableoid("tableoid", "oid")]
#[case::ctid("ctid", "tid")]
#[case::xmin("xmin", "xid")]
#[case::xmax("xmax", "xid")]
#[case::cmin("cmin", "cid")]
#[case::cmax("cmax", "cid")]
fn binds_system_attribute(
    #[case] attribute: &str,
    #[case] ty: &str,
    #[values("", "c.")] qualifier: &str,
) {
    let sql = format!("SELECT {qualifier}{attribute}, $1::integer FROM t AS c");
    let (query, context) = analyze(&sql, Dialect::postgres());
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let result = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        result.col_type(),
        &Some(TypeReference::from_master_type_name(ty))
    );
    assert_eq!(result.is_not_null(), &Some(true));
    let expression = result.expression().unwrap();
    let binding = expression.system_column_reference().unwrap();
    assert_eq!(binding.relation(), &FullName::with_schema("public", "t"));
    assert_eq!(binding.alias().as_str(), "c");
    assert_eq!(binding.attribute().as_str(), attribute);
    assert!(expression.get_column_def().is_none());
    assert!(expression.table_column_reference().is_none());
    assert_eq!(
        context
            .parameter_constraints()
            .result_column(0)
            .unwrap()
            .admissible_types(),
        Some(std::slice::from_ref(&TypeReference::from_master_type_name(
            ty
        )))
    );
}

#[rstest::rstest]
#[case::derived_alias("SELECT c.ctid FROM t c(ctid)", "int4", false)]
#[case::unqualified_alias("SELECT ctid FROM t c(ctid)", "int4", false)]
#[case::other_alias("SELECT c.tableoid FROM t c(renamed)", "oid", true)]
#[case::left_join("SELECT c.tableoid FROM u LEFT JOIN t c ON true", "oid", true)]
#[case::correlated("SELECT (SELECT c.tableoid) FROM t c", "oid", false)]
#[case::projected("SELECT s.tableoid FROM (SELECT tableoid FROM t) s", "oid", false)]
#[case::cte(
    "WITH s AS (SELECT tableoid FROM t) SELECT tableoid FROM s",
    "oid",
    false
)]
#[case::grouped("SELECT c.tableoid FROM t c GROUP BY tableoid", "oid", true)]
fn preserves_lookup_and_type_rules(#[case] sql: &str, #[case] ty: &str, #[case] direct: bool) {
    let (query, context) = analyze(sql, Dialect::postgres());
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let result = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        result.col_type(),
        &Some(TypeReference::from_master_type_name(ty))
    );
    assert_eq!(
        result
            .expression()
            .unwrap()
            .system_column_reference()
            .is_some(),
        direct
    );
    if sql.contains("LEFT JOIN") {
        assert_eq!(result.is_not_null(), &Some(false));
    }
}

#[rstest::rstest]
#[case::unknown("SELECT c.unknown_system FROM t c", "column_not_found_in_table")]
#[case::case_sensitive(r#"SELECT c."TABLEOID" FROM t c"#, "column_not_found_in_table")]
#[case::view("SELECT c.tableoid FROM v c", "column_not_found_in_view")]
#[case::derived(
    "SELECT c.tableoid FROM (SELECT 1 AS id) c",
    "column_not_found_in_sub_query"
)]
#[case::cte(
    "WITH c AS (SELECT 1 AS id) SELECT c.tableoid FROM c",
    "column_not_found_in_cte"
)]
#[case::hidden("SELECT t.tableoid FROM t c", "undefined_alias")]
#[case::absent("SELECT tableoid", "column_not_found_in_source")]
#[case::ambiguous("SELECT tableoid FROM t, u", "ambiguous_column")]
fn retains_failed_lookup(#[case] sql: &str, #[case] code: &str) {
    let (query, context) = analyze(sql, Dialect::postgres());
    assert_problem(&context, code);
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert!(expression.get_type().is_none());
    assert!(expression.system_column_reference().is_none());
    if code == "ambiguous_column" {
        assert!(matches!(
            expression,
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_))
        ));
    }
}

#[rstest::rstest]
#[case::all("SELECT * FROM t")]
#[case::qualified("SELECT c.* FROM t c")]
fn star_excludes_system_attributes(#[case] sql: &str) {
    let (query, context) = analyze(sql, Dialect::postgres());
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(query.result_columns().len(), 1);
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .table_column_reference()
            .is_some()
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::qualified("SELECT c.tableoid FROM t c", "column_not_found_in_table")]
#[case::unqualified("SELECT tableoid FROM t", "column_not_found_in_source")]
fn sqlite_does_not_gain_postgres_attributes(#[case] sql: &str, #[case] code: &str) {
    let (query, context) = analyze(sql, Dialect::sqlite());
    assert_problem(&context, code);
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
}
