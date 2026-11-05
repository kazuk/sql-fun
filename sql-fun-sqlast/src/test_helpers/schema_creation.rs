#[cfg(feature = "sqlite")]
use crate::{
    ast::Dialect,
    context::ParseContext as _,
    names::SchemaName,
    test_helpers::{SqliteSynBuilder, TestParseContext},
};
use crate::{
    ast::StatementNode,
    context::{FrozenBaseContext, MutSourcePathWithSpan as _, SchemaFileContext},
    sem::{AnalysisError, SemAst},
    syn::Node,
    test_helpers::{create_cache_context, ownership::ownership_context, source_span},
};

pub(crate) fn child(parent: FrozenBaseContext) -> SchemaFileContext {
    use crate::context::BaseParseContext;
    let mut context = SchemaFileContext::new(
        std::path::Path::new("creation.sql"),
        &parent,
        parent.get_args(),
        parent.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}
pub(crate) fn postgres(sql: &str) -> Result<SchemaFileContext, AnalysisError> {
    apply(ownership_context(), sql)
}
pub(crate) fn apply(
    mut context: SchemaFileContext,
    sql: &str,
) -> Result<SchemaFileContext, AnalysisError> {
    use crate::ast::TryFromWithTokens;
    let parsed = ::pg_query::parse(sql, ::pg_query::ParserOptions::default())
        .expect("valid PostgreSQL fixture");
    for raw in parsed.protobuf.stmts {
        let node = Node::from(raw.stmt.expect("statement").as_ref().clone());
        let syntax = StatementNode::try_from_with_tokens(&node, &::sql_fun_core::IVec::default())?;
        context =
            SemAst::apply_ast_node(&syntax, context, &mut create_cache_context())?.into_context();
    }
    Ok(context)
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_accepts(first: &str, second: &str) -> Result<bool, ::rusqlite::Error> {
    let connection = ::rusqlite::Connection::open_in_memory()?;
    connection.execute_batch(first)?;
    Ok(connection.execute_batch(second).is_ok())
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> SchemaFileContext {
    let mut root = TestParseContext::default().with_dialect(Dialect::sqlite());
    root.set_search_path(&[SchemaName::from("main")]);
    child(root.freeze())
}
#[cfg(feature = "sqlite")]
pub(crate) fn apply_sqlite(
    context: SchemaFileContext,
    sql: &str,
) -> Result<SchemaFileContext, AnalysisError> {
    let statement = SqliteSynBuilder.parse_as_stmt(sql);
    Ok(SemAst::apply_ast_node(
        &StatementNode::try_from(&statement)?,
        context,
        &mut create_cache_context(),
    )?
    .into_context())
}
