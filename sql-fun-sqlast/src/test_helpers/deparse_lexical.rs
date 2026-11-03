#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
#[cfg(feature = "sqlite")]
use crate::ast::Statement;
#[cfg(feature = "sqlite")]
use crate::parse_statements;

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_statement(sql: &str) -> Statement {
    parse_statements(sql, Dialect::sqlite())
        .next()
        .expect("one statement")
        .expect(sql)
}

#[cfg(feature = "sqlite")]
pub(crate) fn declared_types(sql: &str) -> Vec<String> {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch(sql).unwrap();
    let mut statement = connection.prepare("PRAGMA table_info(t)").unwrap();
    statement
        .query_map([], |row| row.get(2))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
