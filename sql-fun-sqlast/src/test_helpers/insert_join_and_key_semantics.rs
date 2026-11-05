use crate::{
    ast::Dialect,
    parse_statements,
    sem::SemAst,
    test_helpers::{TestParseContext, create_cache_context, insert::catalog},
};

pub(crate) fn analyze_keyed(definition: &str, returning: &str) -> (SemAst, TestParseContext) {
    let mut cache_context = create_cache_context();
    let dialect = Dialect::postgres();
    let mut context = SemAst::from_ast(
        &parse_statements(&format!("CREATE TABLE review579({definition})"), dialect)
            .next()
            .unwrap()
            .unwrap(),
        catalog(dialect),
        &mut cache_context,
    )
    .unwrap()
    .into_context();
    let statement = SemAst::from_ast_node(
        &parse_statements(
            &format!(
                "INSERT INTO review579 VALUES(1) ON CONFLICT(id) DO NOTHING RETURNING {returning}"
            ),
            dialect,
        )
        .next()
        .unwrap()
        .unwrap()
        .into_test_node(),
        &mut context,
        &mut create_cache_context(),
    )
    .unwrap();
    (statement, context)
}
