use crate::ast::{Dialect, DropRule, StatementKind};

use crate::parse_statements;

pub(crate) fn drop_rule(sql: &str, dialect: Dialect) -> DropRule {
    let statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("context-free DROP RULE conversion");
    assert_eq!(statement.kind(), StatementKind::drop_rule());
    statement.into_test_node().into_test_drop_rule()
}
