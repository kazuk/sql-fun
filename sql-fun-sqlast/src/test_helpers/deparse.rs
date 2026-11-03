use crate::ast::{
    DeparseError, Dialect,
    deparse::{SqlEmit, SqlWriter},
};

pub(crate) fn render<T: SqlEmit>(value: &T, dialect: Dialect) -> Result<String, DeparseError> {
    let mut writer = SqlWriter::new(dialect);
    value.emit(&mut writer)?;
    Ok(writer.into_sql())
}
