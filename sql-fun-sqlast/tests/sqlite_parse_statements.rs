//! SQLite parsing through the public statement iterator.

#![cfg(feature = "sqlite")]

use ::sql_fun_sqlast::ast::{Dialect, StatementKind};
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::empty("")]
#[case::whitespace(" \n\t")]
#[case::comments("-- SELECT 1;\n/* INSERT INTO t VALUES (1); */")]
#[case::delimiters(";;; -- trailing\n;")]
fn empty_input_contains_no_statements(#[case] sql: &str) {
    assert!(parse_statements(sql, Dialect::sqlite()).next().is_none());
}

#[test]
fn delimiters_inside_literals_and_comments_do_not_split_statements() {
    let sql = "SELECT 'a;b', 'it''s;quoted'; -- not a statement;\n\
               /* nor this; */ SELECT '日本語;';";
    let statements = parse_statements(sql, Dialect::sqlite())
        .collect::<Result<Vec<_>, _>>()
        .expect("two complete statements");
    assert_eq!(statements.len(), 2);
}

#[test]
fn owned_statements_outlive_the_input_and_iterator() {
    let statements = {
        let sql = String::from("SELECT 'owned'; SELECT 2");
        parse_statements(&sql, Dialect::sqlite())
            .collect::<Result<Vec<_>, _>>()
            .expect("owned ASTs")
    };
    assert_eq!(statements.len(), 2);
}

#[test]
fn syntax_failure_is_emitted_once_and_ends_iteration() {
    let sql = "SELECT '日本語'; SELECT FROM; SELECT 3";
    let mut statements = parse_statements(sql, Dialect::sqlite());
    assert!(statements.next().expect("first statement").is_ok());
    let error = statements
        .next()
        .expect("failed statement")
        .expect_err("syntax error");
    assert!(error.is_syntax());
    assert!(error.is_boundary_discovery());
    assert!(!error.is_recoverable());
    assert_eq!(
        error
            .position()
            .expect("parser-provided token location")
            .seek_pos(),
        std::io::SeekFrom::Start(sql.find("FROM").expect("bad token") as u64)
    );
    assert_eq!(error.span().get_from(sql), Some("SELECT FROM; SELECT 3"));
    assert!(statements.next().is_none());
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::unsupported("PRAGMA user_version;")]
#[case::explain("EXPLAIN SELECT 1;")]
#[case::query_plan("EXPLAIN QUERY PLAN SELECT 1;")]
fn known_boundaries_allow_iteration_after_conversion_failure(#[case] unsupported: &str) {
    let sql = format!("{unsupported} SELECT 2");
    let mut statements = parse_statements(&sql, Dialect::sqlite());
    let error = statements
        .next()
        .expect("unsupported statement")
        .expect_err("conversion error");
    assert!(!error.is_syntax());
    assert!(error.is_recoverable());
    assert!(!error.is_boundary_discovery());
    assert_ne!(error.message(), "");
    assert_eq!(
        error.span().get_from(&sql).expect("statement span").trim(),
        unsupported
    );
    assert!(statements.next().expect("following statement").is_ok());
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::select("WITH c AS (SELECT 1) SELECT * FROM c", StatementKind::query())]
#[case::insert(
    "WITH c AS (SELECT 1) INSERT INTO t SELECT * FROM c",
    StatementKind::insert()
)]
#[case::update(
    "WITH c AS (SELECT 1) UPDATE t SET n = (SELECT * FROM c)",
    StatementKind::update()
)]
#[case::delete(
    "WITH c AS (SELECT 1) DELETE FROM t WHERE n IN (SELECT * FROM c)",
    StatementKind::delete()
)]
fn read_and_write_statements_with_ctes_parse(#[case] sql: &str, #[case] kind: StatementKind) {
    let mut statements = parse_statements(sql, Dialect::sqlite());
    let statement = statements
        .next()
        .expect("single statement")
        .expect("common AST");
    assert_eq!(statement.kind(), kind);
    let clause = statement.with_clause().expect("WITH clause");
    assert_eq!(clause.len(), 1);
    assert_eq!(clause.find("c").expect("catalog CTE").name(), "c");
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::default_values("INSERT INTO items DEFAULT VALUES")]
#[case::replace("INSERT OR REPLACE INTO main.items AS t (id, value) VALUES (1, 2) RETURNING id")]
#[case::chained_upsert(
    "INSERT INTO items(id, value) VALUES (1, 2) ON CONFLICT(id) WHERE id > 0 DO UPDATE SET value = excluded.value WHERE value > 0 ON CONFLICT DO NOTHING RETURNING id"
)]
#[case::tuple_assignment("UPDATE items SET (id, value) = (SELECT 1, 2)")]
fn dml_constructs_are_accepted_without_catalog_resolution(#[case] sql: &str) {
    let statement = parse_statements(sql, Dialect::sqlite())
        .next()
        .expect("single statement")
        .expect("common DML");
    assert!(statement.is_insert() || statement.is_update());
}
