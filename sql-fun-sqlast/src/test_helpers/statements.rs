use crate::{
    AstAndContextPair, StringSpan,
    ast::Dialect,
    context::{FrozenBaseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    parse_statements,
    sem::{AnalysisError, SelectStatement, SemAst, SourcePathWithSpan},
    test_helpers::{TestParseContext, create_cache_context, source_span},
};

pub(crate) fn resolve_postgres(
    sql: &str,
    context: &mut impl ParseContext,
) -> Result<SemAst, AnalysisError> {
    SemAst::from_ast_node(
        &parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
        context,
        &mut create_cache_context(),
    )
}

pub(crate) fn resolve_cursor(sql: &str, context: &mut TestParseContext) -> SemAst {
    resolve_postgres(sql, context).unwrap()
}

pub(crate) fn query_in_context(
    sql: &str,
    context: SchemaFileContext,
) -> Result<(SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    let dialect = context.dialect();
    query(sql, context, dialect)
}

pub(crate) fn query_postgres(
    sql: &str,
    context: SchemaFileContext,
) -> Result<(SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    query(sql, context, Dialect::postgres())
}

fn query(
    sql: &str,
    context: SchemaFileContext,
    dialect: Dialect,
) -> Result<(SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let analyzed = SemAst::from_ast(
        &parse_statements(sql, dialect).next().unwrap()?,
        context,
        &mut cache_context,
    )?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    Ok((query, analyzed.into_context()))
}

/// Analyze one statement with the caller's explicitly selected dialect and provenance.
pub(crate) fn apply(
    mut context: SchemaFileContext,
    sql: &str,
    dialect: Dialect,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    MutSourcePathWithSpan::set_source_path_with_span(&mut context, &source_span());
    SemAst::from_ast(
        &parse_statements(sql, dialect)
            .next()
            .unwrap()
            .expect("valid test SQL"),
        context,
        &mut create_cache_context(),
    )
}

pub(crate) fn layer(base: &FrozenBaseContext, path: &std::path::Path) -> SchemaFileContext {
    use crate::context::BaseParseContext;
    SchemaFileContext::new(path, base, base.get_args(), base.metadata())
}

pub(crate) fn schema_postgres(
    sql: &str,
    mut context: SchemaFileContext,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "schema.sql",
        &StringSpan::new(0, sql.len()),
    ));
    for statement in parse_statements(sql, Dialect::postgres()) {
        context = SemAst::from_ast(&statement?, context, &mut cache_context)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}
