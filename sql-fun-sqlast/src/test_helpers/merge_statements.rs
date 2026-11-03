use crate::ast::Dialect;

use crate::ast::Statement;

use crate::parse_statements;

pub(crate) fn statement(sql: &str) -> Statement {
    parse_statements(sql, Dialect::postgres())
        .next()
        .expect("one statement")
        .expect(sql)
}
