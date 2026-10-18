//! Public syntax/classification boundary for DROP RULE.

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, HaveWithClause, StatementKind};
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::postgres(Dialect::postgres(), "DROP RULE t ON absent")]
fn classifies_without_catalog_and_respects_statement_boundaries(
    #[case] dialect: Dialect,
    #[case] sql: &str,
) {
    let mut statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("syntax conversion without catalog");
    assert_eq!(statement.kind(), StatementKind::drop_rule());
    assert!(statement.with_clause().is_none());
    assert!(
        statement
            .ensure_with_clause()
            .err()
            .expect("unsupported WITH carrier")
            .is_unsupported_carrier()
    );
    assert!(CteDefinition::new("rule", &statement).is_err());
    assert!(statement.deparse_sql(dialect).unwrap_err().is_unsupported());
}

#[rstest::rstest]
#[case::create_trigger(
    "CREATE TRIGGER t AFTER INSERT ON absent EXECUTE FUNCTION f()",
    StatementKind::create_trigger()
)]
#[case::drop_trigger("DROP TRIGGER t ON absent", StatementKind::drop_trigger())]
#[case::create_rule(
    "CREATE RULE r AS ON SELECT TO absent DO INSTEAD NOTHING",
    StatementKind::create_rule()
)]
#[case::drop_rule("DROP RULE r ON absent", StatementKind::drop_rule())]
#[case::drop_table("DROP TABLE absent", StatementKind::drop_table())]
fn classifies_each_statement_independently(#[case] sql: &str, #[case] expected: StatementKind) {
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .expect("statement")
        .expect("context-free syntax conversion");
    assert_eq!(statement.kind(), expected);
}
