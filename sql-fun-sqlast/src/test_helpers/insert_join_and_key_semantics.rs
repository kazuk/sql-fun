use crate::parse_statements;
use crate::test_helpers::create_cache_context;
use crate::test_helpers::insert::catalog;

use crate::ast::Dialect;
use crate::sem::SemAst;
use crate::test_helpers::TestParseContext;

pub(crate) fn analyze_keyed(definition: &str, returning: &str) -> (SemAst, TestParseContext) {
    let mut cache_context = create_cache_context();
    let dialect = Dialect::postgres();
    let mut context = parse_statements(&format!("CREATE TABLE review579({definition})"), dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(catalog(dialect), &mut cache_context)
        .unwrap()
        .into_context();
    let statement = parse_statements(
        &format!(
            "INSERT INTO review579 VALUES(1) ON CONFLICT(id) DO NOTHING RETURNING {returning}"
        ),
        dialect,
    )
    .next()
    .unwrap()
    .unwrap()
    .into_test_node()
    .resolve(&mut context, &mut create_cache_context())
    .unwrap();
    (statement, context)
}
