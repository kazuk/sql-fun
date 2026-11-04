use crate::ast::{
    DeparseError, Dialect,
    deparse::{SqlEmit, SqlWriter},
};
use crate::{ast::Statement, parse_statements};

pub(crate) fn render<T: SqlEmit>(value: &T, dialect: Dialect) -> Result<String, DeparseError> {
    let mut writer = SqlWriter::new(dialect);
    value.emit(&mut writer)?;
    Ok(writer.into_sql())
}

pub(crate) fn statement(sql: &str, dialect: Dialect) -> Statement {
    parse_statements(sql, dialect)
        .next()
        .expect("one statement")
        .expect(sql)
}
