use crate::parse_statements;
use crate::sem::SemAst;
use crate::sem::convert;
use crate::test_helpers::create_cache_context;
use crate::test_helpers::scalar_catalog::items_catalog;

use crate::ast::Dialect;

use crate::test_helpers::TestParseContext;

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
    let result = convert(&statement.into_test_node())
        .resolve(&mut context, &mut create_cache_context())
        .unwrap();
    (result, context)
}
