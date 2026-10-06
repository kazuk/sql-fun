//! Public common AST inspection without parser or catalog dependencies.

use ::sql_fun_sqlast::ast::{Dialect, StatementKind};
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::select(
    "SELECT unknown_function(value) FROM missing_table",
    StatementKind::query()
)]
#[case::values("VALUES (1), (2)", StatementKind::query())]
#[case::union("SELECT 1 UNION SELECT 2", StatementKind::query())]
#[case::insert("INSERT INTO missing_table VALUES (1)", StatementKind::insert())]
#[case::update("UPDATE missing_table SET value = 2", StatementKind::update())]
#[case::delete("DELETE FROM missing_table", StatementKind::delete())]
fn inspects_owned_statements_without_catalog_resolution(
    #[case] sql: &str,
    #[case] expected: StatementKind,
) {
    let statement = {
        let source = sql.to_owned();
        parse_statements(&source, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
    };
    assert_eq!(statement.kind(), expected);
    assert!(statement.with_clause().is_none());
}

#[test]
fn inspects_cte_order_attributes_and_body_without_parser_types() {
    let statement = parse_statements(
        "WITH RECURSIVE first_cte(n) AS NOT MATERIALIZED (SELECT 1), \
         second_cte AS (SELECT n FROM first_cte) SELECT * FROM second_cte",
        Dialect::postgres(),
    )
    .next()
    .unwrap()
    .unwrap();
    let with = statement.with_clause().unwrap();
    assert!(with.is_recursive());
    assert_eq!(with.len(), 2);
    assert_eq!(
        with.iter().map(|entry| entry.name()).collect::<Vec<_>>(),
        ["first_cte", "second_cte"]
    );
    let first = with.find("first_cte").unwrap();
    assert_eq!(first.columns().collect::<Vec<_>>(), ["n"]);
    assert_eq!(first.materialized(), Some(false));
    assert_eq!(first.body_kind(), StatementKind::query());
    assert!(with.find("absent").is_none());
}

#[test]
fn retains_data_modifying_cte_without_returning() {
    let statement = parse_statements(
        "WITH cleanup AS (DELETE FROM missing_table) SELECT 1",
        Dialect::postgres(),
    )
    .next()
    .unwrap()
    .unwrap();
    assert_eq!(
        statement
            .with_clause()
            .unwrap()
            .find("cleanup")
            .unwrap()
            .body_kind(),
        StatementKind::delete()
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::select("SELECT * FROM source", StatementKind::query())]
#[case::insert("INSERT INTO items SELECT n FROM source", StatementKind::insert())]
#[case::update("UPDATE items SET n = (SELECT n FROM source)", StatementKind::update())]
#[case::delete(
    "DELETE FROM items WHERE n IN (SELECT n FROM source)",
    StatementKind::delete()
)]
fn inspects_sqlite_read_and_write_ctes(#[case] body: &str, #[case] kind: StatementKind) {
    let sql = format!("WITH Source(n) AS (SELECT 1) {body}");
    let statement = parse_statements(&sql, Dialect::sqlite())
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(statement.kind(), kind);
    assert!(statement.with_clause().unwrap().find("source").is_some());
}
