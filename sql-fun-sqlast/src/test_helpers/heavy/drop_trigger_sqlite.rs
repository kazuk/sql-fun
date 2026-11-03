use crate::context::{BaseContext, FrozenBaseContext, ParseContext as _, SchemaFileContext};
use crate::test_helpers::drop_trigger_sqlite::{CREATE_AUDIT, apply, layer};

use crate::names::SchemaName;

/// Reload the maintained catalog and analyze the test schema on each call.
/// Catalog state is not shared across independent test processes.
pub(crate) fn catalog() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    use ::clap::Parser;
    let args = ::sql_fun_core::SqlFunArgs::try_parse_from([
        "sql-fun",
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])?;
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "SQLite", "dialect": "SQLite", "version": "3",
        "search_path": ["main"]
    }))?;
    let base = FrozenBaseContext::from(BaseContext::new(&args, &metadata)?);
    let mut context = SchemaFileContext::new(
        std::path::Path::new("drop-trigger.sql"),
        &base,
        &args,
        &metadata,
    );
    for sql in [
        "CREATE TABLE items(id)",
        "CREATE VIEW item_view AS SELECT id FROM items",
        CREATE_AUDIT,
        "CREATE TRIGGER other INSTEAD OF UPDATE ON item_view BEGIN SELECT 1; END",
    ] {
        context = apply(context, sql)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context.freeze())
}

/// Load the maintained catalog and an in-memory SQLite oracle per call.
/// Neither catalog nor connection is shared across test processes.
pub(crate) fn attached_catalog(
    first: &str,
    second: &str,
) -> Result<(FrozenBaseContext, ::rusqlite::Connection), Box<dyn std::error::Error>> {
    let base = catalog()?;
    let mut context = apply(layer(&base), "DROP TRIGGER main.audit")?.into_context();
    context.set_search_path(&[
        SchemaName::from("main"),
        SchemaName::from(first),
        SchemaName::from(second),
    ]);
    let connection = ::rusqlite::Connection::open_in_memory()?;
    for schema in [first, second] {
        connection.execute_batch(&format!("ATTACH DATABASE ':memory:' AS {schema}"))?;
        for sql in [
            format!("CREATE TABLE {schema}.items(id)"),
            format!(
                "CREATE TRIGGER {schema}.audit BEFORE INSERT ON {schema}.items BEGIN SELECT 1; END"
            ),
        ] {
            connection.execute_batch(&sql)?;
            context = apply(context, &sql)?.into_context();
        }
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok((context.freeze(), connection))
}
