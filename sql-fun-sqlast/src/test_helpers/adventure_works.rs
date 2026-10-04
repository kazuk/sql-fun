use crate::sem::{BaseContext, BaseParseContext};
use crate::{PostgresDialect, SqlFunArgs};

use std::path::PathBuf;

/// Load the complete AdventureWorks schema with PostgreSQL 17 and tablefunc.
pub fn adventure_works_schema(
    args: &SqlFunArgs,
) -> Result<impl BaseParseContext, Box<dyn std::error::Error>> {
    let home = PathBuf::from(env!("SQL_FUN_HOME"));
    let builtin = PostgresDialect::parse_schema_file(
        &home.join("postgres/17/schema.sql"),
        BaseContext::new(
            args,
            args.load_metadata()?
                .get("default")
                .expect("selected test metadata"),
        )?,
    )?;
    let extension = PostgresDialect::parse_schema_file(
        &home.join("postgres/17/extension/tablefunc--1.0.sql"),
        builtin,
    )?;
    Ok(PostgresDialect::parse_schema_file(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/adventure-works/schema.develop.sql"),
        extension,
    )?)
}
