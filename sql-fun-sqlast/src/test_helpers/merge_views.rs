use crate::ast::Dialect;
use crate::names::{ColumnName, FullName};
use crate::parse_statements;
use crate::sem::{
    ColumnDefinition, CreateTableAs, CreateView, PgBuiltInType, SemAst, ViewLike, ViewMergeSupport,
    ViewMergeTriggers,
};
use crate::test_helpers::document_merge::catalog;
use crate::test_helpers::{TestParseContext, create_cache_context};

pub(crate) fn view_catalog(version: u32, support: &ViewMergeSupport) -> TestParseContext {
    let mut context = catalog(version);
    install_view(&mut context, false);
    context.set_view_merge_support(&view_name(), support);
    context
}

pub(crate) fn install_view(context: &mut TestParseContext, materialized: bool) {
    let query = SemAst::from_ast_node(
        &parse_statements("SELECT id,label FROM items", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
        context,
        &mut create_cache_context(),
    )
    .unwrap()
    .into_select_statemtnt()
    .unwrap();
    let name = view_name();
    let view = if materialized {
        let definition = CreateTableAs::new(&name, &SemAst::select(&query), true);
        ViewLike::from(&definition)
    } else {
        let definition = CreateView::new(&name, &query, &[], &None, false);
        ViewLike::from(&definition)
    };
    context.set_get_view_impl_result(&name, &Some(view.clone()));
}

pub(crate) fn view_name() -> FullName {
    FullName::with_schema("public", "writable_items")
}

pub(crate) fn automatic_support() -> ViewMergeSupport {
    ViewMergeSupport::new(
        true,
        false,
        ViewMergeTriggers::default(),
        Some(&all_columns()),
        Some(&all_columns()),
    )
}

pub(crate) fn trigger_support(insert: bool, update: bool, delete: bool) -> ViewMergeSupport {
    ViewMergeSupport::new(
        false,
        false,
        ViewMergeTriggers::new(insert, update, delete),
        Some(&all_columns()),
        Some(&all_columns()),
    )
}

pub(crate) fn all_columns() -> Vec<ColumnDefinition> {
    columns(&["id", "label"])
}

pub(crate) fn columns(names: &[&str]) -> Vec<ColumnDefinition> {
    names
        .iter()
        .map(|name| {
            let type_ref = if *name == "id" {
                PgBuiltInType::int4()
            } else {
                PgBuiltInType::text()
            };
            ColumnDefinition::new(
                &Some(ColumnName::from(*name)),
                Some(&type_ref),
                Some(true),
                &[],
            )
        })
        .collect()
}

pub(crate) fn column_definition(name: &str) -> ColumnDefinition {
    ColumnDefinition::new(
        &Some(ColumnName::from(name)),
        Some(&PgBuiltInType::int4()),
        Some(true),
        &[],
    )
}
