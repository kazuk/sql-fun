//! Small access-analysis fixture; no builtin loading or external services.

use crate::{
    AstAndContextPair,
    ast::Dialect,
    names::{ColumnName, FullName, SchemaName},
    parse_statements,
    sem::{
        ColumnDefinition, CreateTableAs, CreateView, OverloadVariant, PgBuiltInType, QueryAccess,
        SemAst, ViewLike,
    },
    test_helpers::{TestParseContext, create_cache_context, source_span},
};

pub(crate) fn context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(PgBuiltInType::int4());
    context.setup_type(PgBuiltInType::bool());
    for name in ["a", "b"] {
        context.setup_table(
            name,
            &[
                ColumnDefinition::new(
                    &Some(ColumnName::from("id")),
                    Some(&PgBuiltInType::int4()),
                    Some(true),
                    &[],
                ),
                ColumnDefinition::new(
                    &Some(ColumnName::from("value")),
                    Some(&PgBuiltInType::int4()),
                    Some(false),
                    &[],
                ),
            ],
            &source_span(),
        );
    }
    context.setup_binary("=", "int4", "int4", "bool", true);
    context
}

pub(crate) fn analyze(sql: &str) -> QueryAccess {
    analyze_in(sql, context(), Dialect::postgres())
}

pub(crate) fn analyze_in(sql: &str, context: TestParseContext, dialect: Dialect) -> QueryAccess {
    let pair = analyze_pair(sql, context, dialect);
    assert_eq!(
        pair.context().reported_problems().len(),
        0,
        "{sql}: {:?}",
        pair.context().reported_problems()
    );
    QueryAccess::from_statement(pair.context(), pair.ast()).unwrap()
}

fn analyze_pair(
    sql: &str,
    context: TestParseContext,
    dialect: Dialect,
) -> AstAndContextPair<TestParseContext> {
    SemAst::from_ast(
        &parse_statements(sql, dialect).next().unwrap().unwrap(),
        context,
        &mut create_cache_context(),
    )
    .unwrap()
}
pub(crate) fn analyze_preserving_output(sql: &str) -> QueryAccess {
    let pair = analyze_pair(sql, context(), Dialect::postgres());
    assert!(pair.context().reported_problems().is_empty());
    let before = pair.ast().result_columns().unwrap().clone();
    let accesses = QueryAccess::from_statement(pair.context(), pair.ast()).unwrap();
    assert_eq!(pair.ast().result_columns(), Some(&before));
    accesses
}

pub(crate) fn analyze_diagnosed(sql: &str) -> (QueryAccess, Vec<String>) {
    let pair = analyze_pair(sql, context(), Dialect::postgres());
    let problems = pair
        .context()
        .reported_problems()
        .iter()
        .map(|problem| problem.code().to_owned())
        .collect();
    (
        QueryAccess::from_statement(pair.context(), pair.ast()).unwrap(),
        problems,
    )
}

pub(crate) fn object_context() -> TestParseContext {
    let mut context = context();
    let statement = parse_statements("SELECT id FROM a", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let pair = SemAst::from_ast(&statement, context, &mut create_cache_context()).unwrap();
    let query = pair.ast().select_statement().unwrap().clone();
    context = pair.into_context();
    let name = FullName::with_schema("public", "v");
    let view = CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    let name = FullName::with_schema("public", "m");
    let materialized = CreateTableAs::new(&name, &SemAst::select(&query), true);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&materialized)));
    let function = OverloadVariant::new(&Some(PgBuiltInType::int4()), &vec![], false, false);
    let name = context.setup_function("f", std::slice::from_ref(&function));
    context.set_function_returns_set(&name, &function, true);
    context
}
