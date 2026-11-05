use crate::ast::Dialect;

use crate::names::{FullName, SchemaName};
use crate::sem::TypeReference;
use crate::test_helpers::TestParseContext;

pub(crate) fn setup_context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "integer"),
        false,
    ));
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "text"),
        false,
    ));
    context
}

#[cfg(feature = "sqlite")]
use crate::{SqliteDialect, SqliteToken, syn::sqlite::CreateTableStatement};
#[cfg(feature = "sqlite")]
use ::sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
#[rstest::fixture]
pub(crate) fn create_table_statement<'a>() -> (CreateTableStatement, IVec<SqliteToken<'a>>) {
    let sql = r#"
                create table users (
                    id integer primary key not null unique,
                    name text default ('guest') collate nocase,
                    active integer check (true and true),
                    created_at text default current_timestamp)
            "#;
    let cmd = SqliteDialect::parse(sql)
        .expect("sqlite parse should succeed")
        .expect("statement should exist");
    let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
    let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
    let Some(create_table) = stmt.into_create_table() else {
        panic!("expected create table statement")
    };
    (create_table, tokens)
}

#[cfg(feature = "sqlite")]
pub(crate) fn parse_create_table<'a>(
    sql: &'a str,
) -> (CreateTableStatement, IVec<SqliteToken<'a>>) {
    let cmd = SqliteDialect::parse(sql)
        .expect("sqlite parse should succeed")
        .expect("statement should exist");
    let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
    let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
    let Some(create_table) = stmt.into_create_table() else {
        panic!("expected create table statement")
    };
    (create_table, tokens)
}
