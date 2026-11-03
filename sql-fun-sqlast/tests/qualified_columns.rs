//! Qualified column binding through the public semantic-analysis boundary.

// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use crate::test_helpers::heavy::qualified_columns as heavy;
use crate::test_helpers::qualified_columns::assert_problem_code;
use ::sql_fun_sqlast::names::FullName;

#[rstest::rstest]
#[case::schema("SELECT a.t.id FROM a.t", "a", "t")]
#[case::search_path("SELECT a.t.id FROM t", "a", "t")]
#[case::database("SELECT chosen.a.t.id FROM a.t", "a", "t")]
#[case::aliased_other_occurrence("SELECT a.t.id FROM a.t, a.t AS b", "a", "t")]
#[case::quoted(r#"SELECT "a.b"."t.x".id FROM "a.b"."t.x""#, "a.b", "t.x")]
fn binds_canonical_table_identity(
    #[case] sql: &str,
    #[case] schema: &str,
    #[case] table: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, Some("chosen"))?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let result = query.result_columns().get_column_at(0).unwrap();
    assert!(result.col_type().is_some());
    let binding = result
        .expression()
        .unwrap()
        .table_column_reference()
        .expect("bound table column");
    assert_eq!(binding.relation(), &FullName::with_schema(schema, table));
    Ok(())
}

#[test]
fn same_named_relations_keep_distinct_qualified_bindings() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT a.t.id, b.t.id FROM a.t, b.t", None)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, schema, expected_type) in [
        (0, "a", ::sql_fun_sqlast::sem::PgBuiltInType::int4()),
        (1, "b", ::sql_fun_sqlast::sem::PgBuiltInType::int8()),
    ] {
        let result = query.result_columns().get_column_at(index).unwrap();
        let binding = result
            .expression()
            .unwrap()
            .table_column_reference()
            .expect("distinct qualified relation binding");
        assert_eq!(binding.relation(), &FullName::with_schema(schema, "t"));
        assert_eq!(result.col_type(), &Some(expected_type));
    }
    Ok(())
}

#[rstest::rstest]
#[case::alias("SELECT a.t.id FROM a.t AS other", "column_source_not_found")]
#[case::same_alias("SELECT a.t.id FROM a.t AS t", "column_source_not_found")]
#[case::absent_from("SELECT a.t.id", "column_source_not_found")]
#[case::wrong_schema("SELECT b.t.id FROM a.t", "column_source_not_found")]
#[case::cte(
    "WITH t AS (SELECT 1 AS id) SELECT a.t.id FROM t",
    "column_source_not_found"
)]
#[case::derived("SELECT a.t.id FROM (SELECT 1 AS id) AS t", "column_source_not_found")]
#[case::join_alias(
    "SELECT a.t.id FROM (a.t JOIN a.u ON true) AS j",
    "column_source_not_found"
)]
#[case::duplicate_source("SELECT a.t.id FROM a.t, a.t", "column_source_ambiguous")]
#[case::cross_database("SELECT other.a.t.id FROM a.t", "column_cross_database")]
fn retains_failed_qualified_lookup(
    #[case] sql: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, Some("chosen"))?;
    assert_problem_code(analyzed.context(), code);
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let result = query.result_columns().get_column_at(0).unwrap();
    let failure = result
        .expression()
        .unwrap()
        .qualification_failure()
        .expect("completed failed lookup");
    assert_eq!(failure.code(), code);
    assert_eq!(failure.column().to_string(), "id");
    assert!(result.col_type().is_none());
    Ok(())
}

#[test]
fn catalog_qualification_requires_explicit_database_identity() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT chosen.a.t.id FROM a.t", None)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let failure = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .qualification_failure()
        .unwrap();
    assert_eq!(failure.code(), "column_database_name_required");
    assert_eq!(
        failure.qualifier(),
        &FullName::with_catalog("chosen", "a", "t")
    );
    Ok(())
}

#[rstest::rstest]
#[case::insert("INSERT INTO a.u(id) VALUES(1) RETURNING a.u.id")]
#[case::update("UPDATE a.u SET id = a.u.id RETURNING a.u.id")]
#[case::delete("DELETE FROM a.u WHERE a.u.id = 1 RETURNING a.u.id")]
fn dml_consumes_the_same_qualified_bindings(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, Some("chosen"))?;
    let ast = analyzed.ast();
    let columns = if let Some(statement) = ast.as_insert() {
        statement.result_columns()
    } else if let Some(statement) = ast.as_update() {
        statement.result_columns()
    } else {
        ast.as_delete().unwrap().result_columns()
    };
    let reference = columns
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .table_column_reference()
        .unwrap();
    assert_eq!(reference.relation(), &FullName::with_schema("a", "u"));
    Ok(())
}

#[test]
fn missing_column_retains_the_selected_relation() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT a.t.absent FROM a.t", None)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let result = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        result
            .expression()
            .unwrap()
            .table_column_reference()
            .unwrap()
            .relation(),
        &FullName::with_schema("a", "t")
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::plain("SELECT main.t.id FROM t", true)]
#[case::alias("SELECT main.a.id FROM t AS a", true)]
#[case::same_alias("SELECT main.t.id FROM t AS t", true)]
#[case::parenthesized_alias("SELECT main.a.id FROM (t) AS a", true)]
#[case::parenthesized_hidden("SELECT main.t.id FROM (t) AS a", false)]
#[case::case_insensitive("SELECT MAIN.T.ID FROM t", true)]
#[case::hidden("SELECT main.t.id FROM t AS a", false)]
#[case::absent("SELECT main.t.id", false)]
#[case::wrong_database("SELECT other.t.id FROM t", false)]
fn sqlite_matches_its_database_and_alias_namespace(
    #[case] sql: &str,
    #[case] valid: bool,
) -> testresult::TestResult {
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE t(id INTEGER NOT NULL); INSERT INTO t VALUES(1)")?;
    assert_eq!(oracle.prepare(sql).is_ok(), valid);
    let analyzed = heavy::analyze_sqlite(sql)?;
    assert_eq!(
        analyzed.context().problems().is_empty(),
        valid,
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let value = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    if valid {
        assert_eq!(
            value.table_column_reference().unwrap().relation(),
            &FullName::with_schema("main", "t")
        );
    } else {
        assert!(value.qualification_failure().is_some());
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_row_images_do_not_expose_database_qualified_relations() -> testresult::TestResult {
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE t(id INTEGER)")?;
    let sql =
        "CREATE TRIGGER tr AFTER INSERT ON t WHEN main.new.id IS NOT NULL BEGIN SELECT 1; END";
    oracle.execute_batch(sql)?;
    assert!(oracle.execute("INSERT INTO t VALUES(1)", []).is_err());
    let analyzed = heavy::analyze_sqlite(sql)?;
    assert_problem_code(analyzed.context(), "column_source_not_found");
    Ok(())
}
