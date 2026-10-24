use crate::sem::{
    CastContext, CastDefinition, CreateDomain, FullName, PgBuiltInType, TypeCategory,
    TypeDefinition, TypeReference,
};
use crate::test_helpers::TestParseContext;

pub(super) fn context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::text(),
    ] {
        context.setup_type(ty.clone());
        let category = if ty == PgBuiltInType::text() {
            'S'
        } else {
            'N'
        };
        context.set_get_type_impl_result(
            ty.full_name(),
            &TypeDefinition::builtin_with_category(
                ty.full_name(),
                Some(TypeCategory::try_from(category).unwrap()),
            ),
        );
    }
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.add_implicit_castable(&PgBuiltInType::int4(), &PgBuiltInType::int8());
    context
}

/// Small explicit catalog for domain identity and array conversion relationships.
pub(super) fn domain_context() -> TestParseContext {
    let mut context = context();
    let amount = TypeReference::from_full_name(FullName::with_schema("public", "amount"));
    let amount_array =
        TypeReference::from_full_name(FullName::with_schema("public", "amount_array"));
    for (name, base) in [
        ("amount", PgBuiltInType::int4()),
        ("nested_amount", amount),
        ("amount_array", PgBuiltInType::int4().array_type()),
        ("nested_amount_array", amount_array),
    ] {
        let ty = TypeReference::from_full_name(FullName::with_schema("public", name));
        let definition = TypeDefinition::from(&CreateDomain::new(&ty, &Some(base)));
        context.set_get_type_impl_result(ty.full_name(), &definition);
    }
    context
}
