use crate::{
    ast::Dialect,
    parse_statements,
    sem::SemAst,
    test_helpers::{TestParseContext, create_cache_context, scalar_catalog::items_catalog},
};

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_accepts(sql: &str) {
    let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
    oracle
        .execute_batch("CREATE TABLE items(id INTEGER, label TEXT);")
        .unwrap();
    assert!(oracle.prepare(sql).is_ok(), "SQLite rejected {sql}");
}
pub(crate) fn catalog(dialect: Dialect) -> TestParseContext {
    items_catalog(dialect, Some(false))
}
pub(crate) fn analyze(sql: &str, dialect: Dialect) -> (SemAst, TestParseContext) {
    let mut context = catalog(dialect);
    let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
    let result = SemAst::from_ast_node(
        &statement.into_test_node(),
        &mut context,
        &mut create_cache_context(),
    )
    .unwrap();
    (result, context)
}
