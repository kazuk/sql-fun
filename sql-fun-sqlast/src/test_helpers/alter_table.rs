use crate::SqliteDialect;
#[cfg(feature = "sqlite")]
use crate::SqliteToken;
#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
use crate::names::{ColumnName, FullName, SchemaName};
use crate::sem::{ColumnDefinition, TypeReference};
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::AlterTable as SqliteAlterTable;
use crate::test_helpers::TestParseContext;
use ::sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
pub(crate) fn parse_sqlite_alter_table<'a>(
    sql: &'a str,
) -> (SqliteAlterTable, IVec<SqliteToken<'a>>) {
    let cmd = SqliteDialect::parse(sql)
        .expect("sqlite parse should succeed")
        .expect("statement should exist");
    let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
    let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
    let alter_table = stmt.into_alter_table().expect("ALTER TABLE statement");
    (alter_table, tokens)
}

#[cfg(feature = "sqlite")]
pub(crate) fn make_column(name: &str) -> ColumnDefinition {
    ColumnDefinition::new(&Some(ColumnName::from(name)), None, None, &[])
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "text"),
        false,
    ));
    context
}
