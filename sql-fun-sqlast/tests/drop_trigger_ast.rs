//! Public syntax/classification boundary for DROP TRIGGER.

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, HaveWithClause, StatementKind};
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::postgres(Dialect::postgres(), "DROP TRIGGER t ON absent")]
#[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite(), "DROP TRIGGER t"))]
fn classifies_without_catalog_and_respects_statement_boundaries(
    #[case] dialect: Dialect,
    #[case] sql: &str,
) {
    let mut statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("syntax conversion without catalog");
    assert_eq!(statement.kind(), StatementKind::drop_trigger());
    assert!(statement.with_clause().is_none());
    assert!(
        statement
            .ensure_with_clause()
            .err()
            .expect("unsupported WITH carrier")
            .is_unsupported_carrier()
    );
    assert!(CteDefinition::new("trigger", &statement).is_err());
    assert!(statement.deparse_sql(dialect).unwrap_err().is_unsupported());
}
