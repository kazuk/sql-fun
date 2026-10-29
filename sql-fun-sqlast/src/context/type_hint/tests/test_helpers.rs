use crate::{
    ast::Dialect,
    context::{BaseParseContext, FrozenBaseContext, ParseContext, SchemaFileContext},
    sem::{SchemaName, TypeReference},
    test_helpers::TestParseContext,
};

/// A small explicit catalog for parameter typing; no builtin catalog loading.
pub(super) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for name in ["int4", "unknown", "text", "bool"] {
        let ty = TypeReference::from_master_type_name(name);
        base.setup_type(ty.clone());
        base.add_implicit_castable(&ty, &ty);
    }
    base.setup_binary("+", "int4", "int4", "int4", true);
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    layer(&base.freeze(), &args, &metadata)
}

pub(super) fn layer(
    base: &FrozenBaseContext,
    args: &::sql_fun_core::SqlFunArgs,
    metadata: &::sql_fun_core::SqlFunMetadata,
) -> SchemaFileContext {
    SchemaFileContext::new(std::path::Path::new("query.sql"), base, args, metadata)
}

pub(super) fn analyze(
    sql: &str,
    context: SchemaFileContext,
) -> crate::AstAndContextPair<SchemaFileContext> {
    crate::parse_statements(sql, context.dialect())
        .next()
        .unwrap()
        .unwrap()
        .analyze(context)
        .unwrap()
}
