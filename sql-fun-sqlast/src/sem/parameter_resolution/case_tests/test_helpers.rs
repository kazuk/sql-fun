use crate::{
    ast::Dialect,
    sem::{BaseParseContext, ParseContext, PgBuiltInType, SchemaFileContext, SchemaName},
    test_helpers::TestParseContext,
};

/// Integer/boolean identity conversions and one integer equality signature.
/// No builtin catalog parsing is needed for these condition/structure contracts.
pub(super) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

#[cfg(feature = "sqlite")]
pub(super) fn sqlite_context() -> SchemaFileContext {
    context_for(Dialect::sqlite())
}

fn context_for(dialect: Dialect) -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(dialect);
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        base.setup_type(ty);
    }
    base.setup_binary("=", "int4", "int4", "bool", true);
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}
