use crate::AstAndContextPair;
use crate::context::{BaseParseContext as _, ParseContext};
use crate::parse_statements;
use crate::sem::{CreateTableAs, CreateView, SelectStatement, SemAst, ViewLike};
use crate::test_helpers::{create_cache_context, source_span};

use crate::ast::Dialect;
use crate::context::{FrozenBaseContext, SchemaFileContext};
use crate::names::{ColumnName, FullName, SchemaName, TableName};
use crate::sem::{
    AnalysisError, ColumnDefinition, CreateTable, CreateTableMode, FunctionOverloadCollection,
    OverloadVariant, PgBuiltInType, TypeReference,
};
use crate::test_helpers::TestParseContext;

pub(crate) fn bound_sources(query: &mut SelectStatement) -> Result<Vec<String>, AnalysisError> {
    #[derive(Default)]
    struct BoundSources(Vec<String>);
    impl crate::sem::traversal::SemanticVisitorMut for BoundSources {
        fn relation_identity(&mut self, name: &FullName) {
            self.0.push(name.to_string());
        }
        fn relation(&mut self, name: Option<&str>, _alias: &str, _columns: &[String]) {
            if let Some(name) = name {
                self.0.push(name.to_owned());
            }
        }
    }
    let mut sources = BoundSources::default();
    query.walk(&mut sources)?;
    Ok(sources.0)
}
pub(crate) fn catalog(dialect: Dialect) -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(dialect);
    let schema = if dialect == Dialect::sqlite() {
        "main"
    } else {
        "public"
    };
    context.set_get_search_path_result(&[SchemaName::from(schema)]);
    add_tables(&mut context, schema);
    let statement = parse_statements("SELECT id FROM items", dialect)
        .next()
        .unwrap()
        .unwrap()
        .into_test_node()
        .resolve(&mut context, &mut create_cache_context())
        .unwrap();
    let query = statement.into_select_statemtnt().unwrap();
    let view_name = FullName::with_schema(schema, "item_view");
    let view = CreateView::new(&view_name, &query, &[], &None, false);
    context.set_get_view_impl_result(&view_name, &Some(ViewLike::from(&view)));
    let materialized_name = FullName::with_schema(schema, "item_matview");
    let materialized = CreateTableAs::new(&materialized_name, &SemAst::select(&query), true);
    context.set_get_view_impl_result(&materialized_name, &Some(ViewLike::from(&materialized)));

    let overload = OverloadVariant::new(
        &Some(TypeReference::from_full_name(FullName::with_schema(
            "pg_catalog",
            "trigger",
        ))),
        &Vec::new(),
        false,
        false,
    );
    for (name, preserves) in [("audit_fn", true), ("set_trigger_fn", false)] {
        let name = FullName::with_schema("public", name);
        context.set_get_function_impl_result(
            &name,
            &FunctionOverloadCollection::new(std::slice::from_ref(&overload)),
        );
        context.set_function_row_property(&name, &overload, preserves);
    }
    let predicate = OverloadVariant::new(&Some(PgBuiltInType::bool()), &Vec::new(), false, false);
    for (name, preserves) in [("predicate_fn", true), ("aggregate_fn", false)] {
        let name = FullName::with_schema("public", name);
        context.set_get_function_impl_result(
            &name,
            &FunctionOverloadCollection::new(std::slice::from_ref(&predicate)),
        );
        context.set_function_row_property(&name, &predicate, preserves);
    }
    context
}
pub(crate) fn add_tables(context: &mut TestParseContext, schema: &str) {
    for name in ["items", "other"] {
        let name = FullName::with_schema(schema, name);
        let column = ColumnDefinition::new(
            &Some(ColumnName::from("id")),
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
        context.set_get_table_impl_result(&name, &Some(table));
    }
}
pub(crate) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
    SchemaFileContext::new(
        std::path::Path::new("trigger.sql"),
        base,
        base.get_args(),
        base.metadata(),
    )
}
pub(crate) fn resolve(sql: &str, context: &mut TestParseContext) -> Result<SemAst, AnalysisError> {
    parse_statements(sql, ParseContext::dialect(context))
        .next()
        .unwrap()
        .unwrap()
        .into_test_node()
        .resolve(context, &mut create_cache_context())
}
pub(crate) fn apply(
    sql: &str,
    context: SchemaFileContext,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    parse_statements(sql, ParseContext::dialect(&context))
        .next()
        .unwrap()
        .unwrap()
        .into_test_node()
        .resolve_and_apply(context, &mut create_cache_context())
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_oracle() -> Result<::rusqlite::Connection, ::rusqlite::Error> {
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE items(id INTEGER); CREATE TABLE other(id INTEGER);")?;
    Ok(oracle)
}
