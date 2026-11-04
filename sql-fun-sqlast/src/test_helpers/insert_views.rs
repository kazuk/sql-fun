use crate::context::{BaseParseContext as _, ParseContext as _};
use crate::names::{SchemaName, TableName};
use crate::parse_statements;
use crate::sem::convert;
use crate::sem::{CreateView, InsertArbiter, InsertInferenceModifiers};
use crate::test_helpers::create_cache_context;
use crate::test_helpers::insert::catalog;

use crate::ast::Dialect;
use crate::names::{ColumnName, FullName};
use crate::sem::{AnalysisError, SemAst, ViewInsertSupport, ViewLike};
use crate::test_helpers::TestParseContext;

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_oracle() -> ::rusqlite::Connection {
    let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
    oracle.execute_batch("CREATE TABLE items(id INTEGER, label TEXT); CREATE VIEW writable_items AS SELECT id,label FROM items; CREATE TRIGGER writable_items_insert INSTEAD OF INSERT ON writable_items BEGIN INSERT INTO items VALUES(new.id,new.label); END;").unwrap();
    oracle
}

pub(crate) fn analyze(
    sql: &str,
    dialect: Dialect,
    writable: bool,
    all_columns: bool,
) -> (Result<SemAst, AnalysisError>, TestParseContext) {
    let mut context = catalog(dialect);
    let view = convert(
        &parse_statements("SELECT id, label FROM items", dialect)
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
    )
    .resolve(&mut context, &mut create_cache_context())
    .unwrap();
    let query = view.into_select_statemtnt().unwrap();
    let name = FullName::with_schema("public", "writable_items");
    let view = CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    let columns = if all_columns {
        vec![ColumnName::from("id"), ColumnName::from("label")]
    } else {
        vec![ColumnName::from("id")]
    };
    let support = if sql.contains("/* identity */") {
        let definition = context
            .get_table(&TableName::from(FullName::with_schema(
                "public",
                "identity_items",
            )))
            .unwrap()
            .get_column_at(0)
            .unwrap()
            .clone();
        ViewInsertSupport::writable(&[definition], true, true)
    } else if sql.contains("/* rule */") {
        ViewInsertSupport::writable(
            &columns
                .iter()
                .map(|name| view.get_column_def(name).unwrap())
                .collect::<Vec<_>>(),
            false,
            false,
        )
    } else if writable {
        ViewInsertSupport::writable(
            &columns
                .iter()
                .map(|name| view.get_column_def(name).unwrap())
                .collect::<Vec<_>>(),
            true,
            dialect == Dialect::postgres(),
        )
    } else {
        ViewInsertSupport::read_only()
    };
    context.set_view_insert_support(&name, &support);
    // A later same-named table must not hide the earlier view.
    let later_table = context
        .get_table(&TableName::from(FullName::with_schema("public", "items")))
        .unwrap()
        .clone();
    context.set_get_table_impl_result(
        &FullName::with_schema("later", "writable_items"),
        &Some(later_table),
    );
    context.set_search_path(&[SchemaName::from("public"), SchemaName::from("later")]);
    if sql.contains("ON CONFLICT(id)") && dialect == Dialect::postgres() {
        context.expect_conflict_target(&TableName::from(FullName::with_schema("public", "items")));
        context.expect_conflict_modifiers(
            &[InsertInferenceModifiers::new(None, None)],
            &[InsertArbiter::index(&FullName::with_schema(
                "public",
                "items_id_key",
            ))],
        );
    }
    let result = convert(
        &parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
    )
    .resolve(&mut context, &mut create_cache_context());
    (result, context)
}
