//! Lightweight domain catalog and operator fixtures; no builtin loading.
use crate::{
    context::{ParseContext, SchemaFileContext, TypeInfoWrite},
    names::FullName,
    sem::{CastContext, CastDefinition, CreateDomain, PgBuiltInType, TypeReference},
    test_helpers::TestParseContext,
};

pub(crate) fn reference(name: &str) -> TypeReference {
    TypeReference::from_full_name(FullName::with_schema("public", name))
}

pub(crate) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default();
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::numeric(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        base.setup_type(ty);
    }
    base.set_get_implicit_cast_result(
        &PgBuiltInType::numeric(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::explicit())),
    );
    base.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    base.set_get_implicit_cast_result(
        &PgBuiltInType::int8(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::assignment())),
    );
    base.set_get_implicit_castable_result(
        &PgBuiltInType::int4(),
        &std::collections::HashSet::from([PgBuiltInType::int4(), PgBuiltInType::int8()]),
    );
    crate::test_helpers::schema_layer(std::path::Path::new("domains.sql"), &base.freeze())
}

pub(crate) fn domain(
    context: &mut SchemaFileContext,
    name: &str,
    base: Option<&TypeReference>,
) -> TypeReference {
    let reference = reference(name);
    let definition = CreateDomain::new(&reference, &base.cloned());
    context.insert_create_domain(
        reference.full_name().expect("nominal type identity"),
        &definition,
    );
    reference
}

pub(crate) fn operators(domain: &TypeReference) -> Vec<crate::sem::BinaryOperatorDefinition> {
    use crate::sem::BinaryOperatorDefinition;
    vec![
        BinaryOperatorDefinition::new(domain, domain, &PgBuiltInType::text(), true),
        BinaryOperatorDefinition::new(
            &PgBuiltInType::int4(),
            &PgBuiltInType::int4(),
            &PgBuiltInType::bool(),
            true,
        ),
        BinaryOperatorDefinition::new(domain, &PgBuiltInType::int8(), &PgBuiltInType::int8(), true),
    ]
}
