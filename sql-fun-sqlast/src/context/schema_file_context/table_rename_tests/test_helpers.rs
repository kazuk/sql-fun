use crate::{
    context::{
        BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext,
    },
    sem::{AnalysisError, FullName},
    test_helpers::{ownership::apply_ownership_sql, source_span},
};

pub(super) fn context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"postgres", "dialect":"postgres", "version":"17", "search_path":["public"]
    }))
    .unwrap();
    let mut inventory = crate::test_helpers::TestParseContext::default().with_metadata(&metadata);
    inventory.set_search_path(&[crate::sem::SchemaName::from("public")]);
    inventory.setup_type(crate::sem::PgBuiltInType::int4());
    inventory.setup_type(crate::sem::TypeReference::from_master_type_name("trigger"));
    child(&inventory.freeze())
}

pub(super) fn name(local: &str) -> FullName {
    FullName::with_schema("public", local)
}

pub(super) fn child(base: &FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("rename.sql"),
        base,
        base.get_args(),
        base.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

pub(super) fn table_layer(inherited: bool) -> Result<SchemaFileContext, AnalysisError> {
    let context =
        apply_ownership_sql(context(), "CREATE TABLE before_name(id integer)")?.into_context();
    if inherited {
        Ok(child(&context.freeze()))
    } else {
        Ok(context)
    }
}

pub(super) fn conflicting_layer() -> Result<SchemaFileContext, AnalysisError> {
    let context = apply_ownership_sql(table_layer(false)?, "CREATE TABLE occupied(id integer)")?
        .into_context();
    Ok(apply_ownership_sql(context, "CREATE TYPE type_only AS ENUM ('x')")?.into_context())
}

pub(super) fn assert_select(context: SchemaFileContext, sql: &str) -> Result<(), AnalysisError> {
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
pub(super) fn sqlite_context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"SQLite", "dialect":"SQLite", "version":"3", "search_path":["main"]
    }))
    .unwrap();
    let mut inventory = crate::test_helpers::TestParseContext::default()
        .with_metadata(&metadata)
        .with_dialect(crate::ast::Dialect::sqlite());
    inventory.set_search_path(&[crate::sem::SchemaName::from("main")]);
    child(&inventory.freeze())
}
#[cfg(feature = "sqlite")]
pub(super) fn apply_sqlite(
    context: SchemaFileContext,
    sql: &str,
) -> Result<crate::AstAndContextPair<SchemaFileContext>, AnalysisError> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    crate::parse_statements(sql, crate::ast::Dialect::sqlite())
        .next()
        .unwrap()
        .expect("valid SQLite SQL")
        .analyze(context, &mut cache_context)
}
