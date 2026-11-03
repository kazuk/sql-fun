use crate::ast::{Dialect, DropTrigger, StatementKind};
use crate::parse_statements;

pub(crate) fn drop_trigger(sql: &str, dialect: Dialect) -> DropTrigger {
    let statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("context-free DROP TRIGGER conversion");
    assert_eq!(statement.kind(), StatementKind::drop_trigger());
    statement.into_test_node().into_test_drop_trigger()
}
