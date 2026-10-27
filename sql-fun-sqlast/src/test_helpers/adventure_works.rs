use crate::context::BaseParseContext;
use crate::sem::{BaseContext, FrozenBaseContext};
use crate::{PostgresDialect, SqlFunArgs};

use std::path::PathBuf;

/// Load the complete AdventureWorks schema with PostgreSQL 17 and tablefunc.
pub fn adventure_works_schema(
    args: &SqlFunArgs,
) -> Result<impl BaseParseContext, Box<dyn std::error::Error>> {
    let home = PathBuf::from(env!("SQL_FUN_HOME"));
    let collection = args.load_metadata()?;
    let selected = collection.get("default").expect("selected test metadata");
    let base = FrozenBaseContext::from(BaseContext::new(args, selected)?);
    let builtin = PostgresDialect::parse_schema_file(
        &home.join("postgres/17/schema.sql"),
        &base,
        &mut None,
        args,
        selected,
    )?;
    let extension = PostgresDialect::parse_schema_file(
        &home.join("postgres/17/extension/tablefunc--1.0.sql"),
        builtin.base_context(),
        &mut None,
        args,
        selected,
    )?;
    Ok(PostgresDialect::parse_schema_file(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/adventure-works/schema.develop.sql"),
        extension.base_context(),
        &mut None,
        args,
        selected,
    )?)
}
