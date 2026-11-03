use crate::ast::Dialect;
use crate::parse_statements;
use ::rusqlite::types::Value;

pub(crate) fn render(sql: &str) -> String {
    let mut rendered = String::new();
    for statement in parse_statements(sql, Dialect::sqlite()) {
        let statement = statement.expect("common SQLite statement");
        rendered.push_str(
            &statement
                .deparse_sql(Dialect::sqlite())
                .expect("render retained types"),
        );
        rendered.push(';');
    }
    rendered
}

pub(crate) fn query_rows(sql: &str) -> Vec<Vec<Value>> {
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    rows(&connection, sql)
}

pub(crate) fn script_rows(script: &str, sql: &str) -> Vec<Vec<Value>> {
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch(script).expect("SQLite script");
    rows(&connection, sql)
}

pub(crate) fn rows(connection: &::rusqlite::Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut statement = connection.prepare(sql).expect("SQLite query");
    let columns = statement.column_count();
    let mut query = statement.query([]).unwrap();
    let mut values = Vec::new();
    while let Some(row) = query.next().unwrap() {
        let mut cells = Vec::new();
        for column in 0..columns {
            cells.push(row.get(column).unwrap());
        }
        values.push(cells);
    }
    values
}
