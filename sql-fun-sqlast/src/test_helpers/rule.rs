use crate::context::{BaseParseContext as _, MutSourcePathWithSpan};
use crate::names::OperatorName;
use crate::parse_statements;
use crate::sem::{
    BinaryOperatorDefinition, CastContext, CastDefinition, CreateView, OperatorDefinition, ViewLike,
};
use crate::test_helpers::statements::resolve_postgres as resolve;
use crate::test_helpers::{create_cache_context, source_span};

use crate::ast::Dialect;
use crate::context::{FrozenBaseContext, SchemaFileContext};
use crate::names::{ColumnName, FullName, SchemaName, TableName};
use crate::sem::{AnalysisError, ColumnDefinition, CreateTable, CreateTableMode, PgBuiltInType};
use crate::test_helpers::TestParseContext;

pub(crate) fn catalog_version(version: &str) -> TestParseContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))
    .expect("explicit rule fixture version");
    catalog().with_metadata(&metadata)
}
pub(crate) fn catalog() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    MutSourcePathWithSpan::set_source_path_with_span(&mut context, &source_span());
    let int = PgBuiltInType::int4();
    context.set_get_implicit_cast_result(
        &int,
        &int,
        Some(CastDefinition::new(CastContext::NoConversion)),
    );
    let equality = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
        &int,
        &int,
        &PgBuiltInType::bool(),
        true,
    )]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "="),
        true,
        true,
        Some(&equality),
    );
    for name in ["items", "sink"] {
        add_table(&mut context, &FullName::with_schema("public", name));
    }
    let query = resolve("SELECT id FROM items", &mut context)
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "item_view");
    let view = CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    context
}
pub(crate) fn add_table(context: &mut TestParseContext, name: &FullName) {
    add_table_column(context, name, "id");
}
pub(crate) fn add_table_column(context: &mut TestParseContext, name: &FullName, column: &str) {
    let column = ColumnDefinition::new(
        &Some(ColumnName::from(column)),
        Some(&PgBuiltInType::int4()),
        Some(false),
        &[],
    );
    let table = CreateTable::new(
        &TableName::from(name.clone()),
        vec![column],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    );
    context.set_get_table_impl_result(name, &Some(table));
}

pub(crate) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
    SchemaFileContext::new(
        std::path::Path::new("rule.sql"),
        base,
        base.get_args(),
        base.metadata(),
    )
}
pub(crate) fn apply(
    sql: &str,
    context: SchemaFileContext,
) -> Result<SchemaFileContext, AnalysisError> {
    Ok(parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .into_test_node()
        .resolve_and_apply(context, &mut create_cache_context())?
        .into_context())
}
