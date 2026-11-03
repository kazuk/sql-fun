use crate::context::ParseContext as _;

use crate::context::{BaseContext, FrozenBaseContext, SchemaFileContext};
use crate::test_helpers::drop_trigger::{CREATE_AUDIT, apply};
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
        "engine": "PostgreSQL", "dialect": "postgres", "version": "17",
        "search_path": ["public", "pg_catalog"]
    }))?;
    let base = FrozenBaseContext::from(BaseContext::new(&args, &metadata)?);
    let mut context = SchemaFileContext::new(
        std::path::Path::new("drop-trigger.sql"),
        &base,
        &args,
        &metadata,
    );
    for sql in [
        "CREATE TABLE public.items(id integer)",
        "CREATE VIEW public.item_view AS SELECT id FROM public.items",
        "CREATE FUNCTION public.audit_fn() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'",
        CREATE_AUDIT,
        "CREATE TRIGGER other INSTEAD OF UPDATE ON item_view FOR EACH ROW EXECUTE FUNCTION audit_fn()",
    ] {
        context = apply(context, sql)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context.freeze())
}
