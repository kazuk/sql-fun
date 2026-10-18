//! Public syntax/classification boundary for PostgreSQL CREATE RULE.

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, HaveWithClause, StatementKind};
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::select("CREATE RULE r AS ON SELECT TO absent DO INSTEAD SELECT 1")]
#[case::insert("CREATE RULE r AS ON INSERT TO absent DO NOTHING")]
#[case::update("CREATE RULE r AS ON UPDATE TO absent DO (SELECT NEW.id; SELECT OLD.id)")]
#[case::delete("CREATE RULE r AS ON DELETE TO absent WHERE OLD.id > 0 DO INSTEAD NOTHING")]
fn classifies_without_catalog_and_respects_statement_boundaries(#[case] sql: &str) {
    let dialect = Dialect::postgres();
    let mut statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("context-free conversion");
    assert_eq!(statement.kind(), StatementKind::create_rule());
    assert!(statement.with_clause().is_none());
    assert!(
        statement
            .ensure_with_clause()
            .err()
            .expect("unsupported WITH carrier")
            .is_unsupported_carrier()
    );
    assert!(statement.with_clause().is_none());
    assert!(CteDefinition::new("rule", &statement).is_err());
    assert!(statement.deparse_sql(dialect).unwrap_err().is_unsupported());
}
