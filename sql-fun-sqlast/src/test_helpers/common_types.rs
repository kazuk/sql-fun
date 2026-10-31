use crate::sem::{
    CastContext, CastDefinition, CreateDomain, FullName, PgBuiltInType, TypeCategory,
    TypeDefinition, TypeReference,
};
use crate::test_helpers::TestParseContext;

pub(crate) fn context() -> TestParseContext {
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
                None,
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
pub(crate) fn domain_context() -> TestParseContext {
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

/// Catalog for post-analysis assignments, forbidding catalog-wide inference.
pub(crate) fn inspection_context() -> TestParseContext {
    let mut context = assignment_context();
    context.forbid_type_enumeration();
    context
}

/// The maintained protocol inference contract, independent of implementation seeds.
pub(crate) fn protocol_candidates() -> crate::sem::InputTypeConstraint {
    crate::sem::InputTypeConstraint::types(
        [
            "bool", "int4", "int8", "float4", "float8", "text", "numeric",
        ]
        .map(TypeReference::from_master_type_name),
    )
}

/// Catalog for CASE analysis, including SQL-established boolean conditions.
pub(crate) fn assignment_context() -> TestParseContext {
    let mut context = domain_context();
    context.setup_type(TypeReference::from_master_type_name("uuid"));
    context.setup_type(PgBuiltInType::bool());
    context
}
