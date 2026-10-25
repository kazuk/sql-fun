use crate::sem::{
    BaseParseContext, CastContext, CastDefinition, ParseContext, PgBuiltInType, SchemaFileContext,
    SchemaName,
};
use crate::test_helpers::TestParseContext;

pub(super) fn analyze(sql: &str) -> crate::AstAndContextPair<SchemaFileContext> {
    let mut catalog = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
    catalog
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::numeric(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        catalog.setup_type(ty);
    }
    catalog.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::numeric(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    for name in ["=", "<>", "<", ">", "<=", ">="] {
        catalog.setup_binary(name, "int4", "int4", "bool", true);
        catalog.setup_binary(name, "numeric", "numeric", "bool", true);
    }
    let args = catalog.get_args().clone();
    let metadata = catalog.metadata().clone();
    let context = SchemaFileContext::new(
        std::path::Path::new("comparison.sql"),
        &catalog.freeze(),
        &args,
        &metadata,
    );
    crate::parse_statements(sql, crate::ast::Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .analyze(context)
        .unwrap()
}
