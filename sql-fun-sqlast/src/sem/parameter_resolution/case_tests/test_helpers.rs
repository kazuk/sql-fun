use crate::{
    ast::Dialect,
    context::{BaseParseContext, SchemaFileContext},
    names::{FullName, SchemaName},
    sem::{CreateDomain, ParseContext, PgBuiltInType, TypeCategory, TypeDefinition, TypeReference},
    test_helpers::TestParseContext,
};

/// Small scalar/domain catalog with integer equality and addition signatures.
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
        base.setup_type(ty.clone());
        let category = if ty == PgBuiltInType::int4() {
            'N'
        } else if ty == PgBuiltInType::bool() {
            'B'
        } else {
            'S'
        };
        base.set_get_type_impl_result(
            ty.full_name().expect("nominal type identity"),
            &TypeDefinition::builtin_with_category(
                ty.full_name().expect("nominal type identity"),
                Some(TypeCategory::try_from(category).unwrap()),
                None,
            ),
        );
    }
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
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}
