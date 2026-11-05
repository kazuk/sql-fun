#[cfg(feature = "sqlite")]
use crate::{
    AstAndContextPair, ast::Dialect, parse_statements, sem::SemAst,
    test_helpers::create_cache_context,
};

use crate::{
    context::{
        BaseParseContext as _, FrozenBaseContext, MutSourcePathWithSpan as _, ParseContext as _,
        SchemaFileContext,
    },
    names::{FullName, SchemaName},
    sem::{AnalysisError, PgBuiltInType, TypeReference},
    test_helpers::{TestParseContext, ownership::apply_ownership_sql, source_span},
};

pub(crate) fn context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"postgres", "dialect":"postgres", "version":"17", "search_path":["public"]
    }))
    .unwrap();
    let mut inventory = TestParseContext::default().with_metadata(&metadata);
    inventory.set_search_path(&[SchemaName::from("public")]);
    inventory.setup_type(PgBuiltInType::int4());
    inventory.setup_type(TypeReference::from_master_type_name("trigger"));
    child(&inventory.freeze())
}

pub(crate) fn name(local: &str) -> FullName {
    FullName::with_schema("public", local)
}

pub(crate) fn child(base: &FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("rename.sql"),
        base,
        base.get_args(),
        base.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

pub(crate) fn table_layer(inherited: bool) -> Result<SchemaFileContext, AnalysisError> {
    let context =
        apply_ownership_sql(context(), "CREATE TABLE before_name(id integer)")?.into_context();
    if inherited {
        Ok(child(&context.freeze()))
    } else {
        Ok(context)
    }
}

pub(crate) fn conflicting_layer() -> Result<SchemaFileContext, AnalysisError> {
    let context = apply_ownership_sql(table_layer(false)?, "CREATE TABLE occupied(id integer)")?
        .into_context();
    Ok(apply_ownership_sql(context, "CREATE TYPE type_only AS ENUM ('x')")?.into_context())
}

pub(crate) fn assert_select(context: SchemaFileContext, sql: &str) -> Result<(), AnalysisError> {
    let count = context.problems().len();
    let result = apply_ownership_sql(context, sql)?;
    assert_eq!(result.context().problems().len(), count);
    assert!(
        result
            .ast()
            .result_columns()
            .unwrap()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_some()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"SQLite", "dialect":"SQLite", "version":"3", "search_path":["main"]
    }))
    .unwrap();
    let mut inventory = TestParseContext::default()
        .with_metadata(&metadata)
        .with_dialect(Dialect::sqlite());
    inventory.set_search_path(&[SchemaName::from("main")]);
    child(&inventory.freeze())
}
#[cfg(feature = "sqlite")]
pub(crate) fn apply_sqlite(
    context: SchemaFileContext,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    let mut cache_context = create_cache_context();
    SemAst::from_ast(
        &parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()
            .expect("valid SQLite SQL"),
        context,
        &mut cache_context,
    )
}
