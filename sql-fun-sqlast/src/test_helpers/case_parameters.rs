use crate::ast::Dialect;
use crate::test_helpers::scalar_catalog::{categorized_scalar_catalog, schema_from_base};

use crate::context::SchemaFileContext;
use crate::names::FullName;
use crate::sem::CreateDomain;

use crate::sem::{PgBuiltInType, TypeDefinition, TypeReference};

/// Small scalar/domain catalog with integer equality and addition signatures.
/// No builtin catalog parsing is needed for these condition/structure contracts.
pub(crate) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> SchemaFileContext {
    context_for(Dialect::sqlite())
}

fn context_for(dialect: Dialect) -> SchemaFileContext {
    let mut base = categorized_scalar_catalog(dialect);

    for (name, target) in [
        ("amount", PgBuiltInType::int4()),
        ("amount_array", PgBuiltInType::int4().array_type()),
    ] {
        let ty = TypeReference::from_full_name(FullName::with_schema("public", name));
        let definition = TypeDefinition::from(&CreateDomain::new(&ty, &Some(target)));
        base.set_get_type_impl_result(ty.full_name().expect("nominal type identity"), &definition);
    }
    base.setup_binary("+", "int4", "int4", "int4", true);
    base.setup_binary("=", "int4", "int4", "bool", true);
    schema_from_base(base, std::path::Path::new("query.sql"))
}
