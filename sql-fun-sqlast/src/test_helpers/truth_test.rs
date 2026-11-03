use crate::ast::Dialect;
use crate::test_helpers::scalar_catalog::{categorized_scalar_catalog, schema_from_base};
use crate::test_helpers::statements::schema_postgres;

use crate::context::SchemaFileContext;
use crate::names::FullName;
use crate::sem::{CastContext, CastDefinition};

use crate::sem::{PgBuiltInType, TypeReference};

/// Small type catalog with integer equality and configurable text-to-bool casts.
/// These tests need no builtin catalog loading or external services.
pub(crate) fn context(cast: Option<CastContext>) -> SchemaFileContext {
    let mut base = categorized_scalar_catalog(Dialect::postgres());

    if let Some(cast) = cast {
        base.set_get_implicit_cast_result(
            &PgBuiltInType::text(),
            &PgBuiltInType::bool(),
            Some(CastDefinition::new(cast)),
        );
    }
    base.setup_binary("=", "int4", "int4", "bool", true);
    schema_from_base(base, std::path::Path::new("query.sql"))
}

pub(crate) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    schema_postgres(sql, context(None))
}

pub(crate) fn bool_domain() -> TypeReference {
    TypeReference::from_full_name(FullName::with_schema("public", "flag"))
}
