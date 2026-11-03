use crate::ast::Dialect;
use crate::test_helpers::TestParseContext;
use crate::test_helpers::delete::catalog;

pub(crate) fn catalog_for_version(version: &str) -> TestParseContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))
    .unwrap();
    catalog(Dialect::postgres()).with_metadata(&metadata)
}
