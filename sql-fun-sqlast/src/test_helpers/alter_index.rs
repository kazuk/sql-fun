use crate::{
    AstAndContextPair,
    ast::{StatementNode, TryFromWithTokens},
    context::{
        BaseParseContext as _, FrozenBaseContext, IndexInfoRead, MutSourcePathWithSpan as _,
        SchemaFileContext,
    },
    names::FullName,
    sem::{AnalysisError, CreateIndex, SemAst},
    syn::Node,
    test_helpers::{
        create_cache_context,
        ownership::{apply_ownership_sql, ownership_context},
        source_span,
    },
};

pub(crate) fn deduplicate(index: &CreateIndex) -> Option<bool> {
    match index.storage_parameter("deduplicate_items") {
        Some(value) => value.as_boolean(),
        None => None,
    }
}
pub(crate) fn scale(index: &CreateIndex) -> Option<f64> {
    match index.storage_parameter("vacuum_cleanup_index_scale_factor") {
        Some(value) => value.as_real(),
        None => None,
    }
}
pub(crate) fn index_name() -> FullName {
    FullName::with_schema("public", "idx")
}
pub(crate) fn index(context: &impl IndexInfoRead) -> &CreateIndex {
    context.get_index_impl(&index_name()).expect("index")
}
pub(crate) fn named_fillfactor(context: &impl IndexInfoRead, name: &FullName) -> Option<i32> {
    match context
        .get_index_impl(name)
        .unwrap()
        .storage_parameter("fillfactor")
    {
        Some(value) => value.as_integer(),
        None => None,
    }
}
pub(crate) fn fillfactor(context: &impl IndexInfoRead) -> Option<i32> {
    named_fillfactor(context, &index_name())
}
pub(crate) fn catalog() -> Result<SchemaFileContext, AnalysisError> {
    seed(
        "CREATE TABLE items(a integer, b integer); CREATE UNIQUE INDEX idx ON items(a, (1)) INCLUDE (b) WHERE a IS NOT NULL",
    )
}
pub(crate) fn seed(sql: &str) -> Result<SchemaFileContext, AnalysisError> {
    let mut context = ownership_context();
    let parsed =
        ::pg_query::parse(sql, ::pg_query::ParserOptions::default()).expect("valid fixture");
    for raw in parsed.protobuf.stmts {
        let node = Node::from(raw.stmt.expect("statement").as_ref().clone());
        let syntax = StatementNode::try_from_with_tokens(&node, &::sql_fun_core::IVec::default())?;
        context =
            SemAst::apply_ast_node(&syntax, context, &mut create_cache_context())?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}
pub(crate) fn child(parent: &FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("alter-index.sql"),
        parent,
        parent.get_args(),
        parent.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}
pub(crate) fn statement(
    context: SchemaFileContext,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    apply_ownership_sql(context, sql)
}
pub(crate) fn alter(
    context: SchemaFileContext,
    command: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    statement(context, &format!("ALTER INDEX idx {command}"))
}
