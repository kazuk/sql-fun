use crate::test_helpers::create_cache_context;
use ::sql_fun_sqlast::{
    AstAndContextPair,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, ParseContext, SchemaFileContext},
    parse_statements,
    sem::SemAst,
};

use ::sql_fun_core::SqlFunArgs;

use std::path::{Path, PathBuf};

/// Load real builtin cast metadata and small domain DDL per test process.
/// No shared setup.
pub(crate) fn analyze(
    version: &str,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.to_str().unwrap(),
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        version,
    ])?;
    let selected = args.load_metadata()?.remove("default").unwrap();
    let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    let mut context = SchemaFileContext::new(Path::new("domains.sql"), &base, &args, &selected);
    // Minimal real DDL; do not parse the whole catalog for small cases.
    for statement in parse_statements(
        r"
            CREATE SCHEMA information_schema;
            CREATE DOMAIN information_schema.cardinal_number AS integer;
            CREATE DOMAIN information_schema.character_data AS varchar;
            CREATE DOMAIN information_schema.sql_identifier AS name;
            CREATE DOMAIN information_schema.time_stamp AS timestamptz;
            CREATE DOMAIN public.amount AS integer;
            CREATE DOMAIN public.nested_amount AS public.amount;
            CREATE DOMAIN public.amounts AS integer[];
        ",
        Dialect::postgres(),
    ) {
        context = SemAst::from_ast(&statement?, context, &mut cache_context)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    base = context.freeze();
    let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
    Ok(SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
        context,
        &mut cache_context,
    )?)
}
