use crate::{
    ast::Dialect,
    parse_statements,
    sem::{AnalysisError, CastContext, CastDefinition, PgBuiltInType, SemAst},
    test_helpers::{TestParseContext, create_cache_context, scalar_catalog::items_catalog},
};

pub(crate) fn catalog_with_text_integer_cast() -> TestParseContext {
    let mut context = catalog(Dialect::postgres());
    context.set_get_implicit_cast_result(
        &PgBuiltInType::text(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::explicit())),
    );
    context
}
pub(crate) fn catalog(dialect: Dialect) -> TestParseContext {
    items_catalog(dialect, Some(true))
}
pub(crate) fn resolve(
    sql: &str,
    context: &mut TestParseContext,
    dialect: Dialect,
) -> Result<SemAst, AnalysisError> {
    SemAst::from_ast_node(
        &parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
        context,
        &mut create_cache_context(),
    )
}
pub(crate) fn analyze(sql: &str, dialect: Dialect) -> (SemAst, TestParseContext) {
    let mut context = catalog(dialect);
    let result = resolve(sql, &mut context, dialect).unwrap();
    (result, context)
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_accepts(sql: &str) {
    let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
    oracle
        .execute_batch("CREATE TABLE items(id INTEGER NOT NULL, label TEXT);")
        .unwrap();
    assert!(oracle.prepare(sql).is_ok(), "SQLite rejected {sql}");
}
