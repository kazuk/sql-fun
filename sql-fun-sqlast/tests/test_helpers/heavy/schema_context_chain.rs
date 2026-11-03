use crate::test_helpers::heavy;
use crate::test_helpers::schema_context_chain::{load_layer, load_schema};
use ::sql_fun_sqlast::ParsedSchemaDump;
use ::sql_fun_sqlast::context::BaseContext;

use ::sql_fun_sqlast::{PostgresDialect, context::FrozenBaseContext};
use std::path::Path;

/// Load real builtin type/function metadata for index absence and DDL
/// resolution tests. Every call reloads metadata, without sharing across
/// calls or nextest processes; the full builtin schema is not parsed.
pub fn index_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    builtin_root()
}

/// Load builtin JSON metadata and the real tablefunc extension for
/// extension integration coverage. Every call repeats both loads;
/// results are not cached or shared between calls or test processes.
pub fn tablefunc_schema() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    let prefix = builtin_root()?;
    let file = Path::new(env!("SQL_FUN_HOME")).join("postgres/17/extension/tablefunc--1.0.sql");
    Ok({
        let (args, selected) = heavy::environment("17")?;
        let analysis_base = &prefix;
        PostgresDialect::parse_schema_file(&file, analysis_base, &mut None, &args, &selected)
    }?
    .base_context()
    .clone())
}

/// Load real builtin metadata per call for schema-chain resolution.
/// Initialization is repeated across calls and independent test processes.
pub(crate) fn parent_with_same_named_tables(
    with_primary_keys: bool,
) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
    let root = builtin_root()?;
    let mut schema = String::from(
        "CREATE SCHEMA a; CREATE SCHEMA b; \
         CREATE TABLE a.items (id INTEGER, other_id INTEGER, sibling_id INTEGER); \
         CREATE TABLE b.items (id INTEGER, other_id INTEGER, sibling_id INTEGER);",
    );
    if with_primary_keys {
        schema.push_str(
            "ALTER TABLE a.items ADD CONSTRAINT items_pk PRIMARY KEY (id); \
             ALTER TABLE b.items ADD CONSTRAINT items_pk PRIMARY KEY (id);",
        );
    }
    load_layer(&root, &["a", "b"], &schema)
}
/// Load real builtin metadata per call for schema-chain resolution.
/// Initialization is repeated across calls and independent test processes.
pub(crate) fn builtin_context() -> Result<BaseContext, Box<dyn std::error::Error>> {
    // Real type/function/operator/cast metadata is sufficient for small fixtures;
    // only catalog and full-extension tests need the entire builtin schema SQL.
    let (args, selected) = heavy::environment("17")?;
    Ok(BaseContext::new(&args, &selected)?)
}
/// Load real builtin metadata per call for schema-chain resolution.
/// Initialization is repeated across calls and independent test processes.
pub(crate) fn builtin_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    Ok(FrozenBaseContext::from(builtin_context()?))
}
/// Load real builtin metadata per call for schema-chain resolution.
/// Initialization is repeated across calls and independent test processes.
pub(crate) fn chain_prefix() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    let root = builtin_root()?;
    let parsed = load_schema(&root, "CREATE TABLE public.base_items (id integer);")?;
    assert!(parsed.problems().is_empty());
    Ok(parsed.base_context().clone())
}
