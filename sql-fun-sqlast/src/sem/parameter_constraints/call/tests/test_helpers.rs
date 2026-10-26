use crate::sem::parameter_resolution::Signature;
use crate::sem::{PgBuiltInType, TypeReference};
use crate::test_helpers::TestParseContext;

pub(super) fn comparison() -> (TestParseContext, Vec<Signature>) {
    let mut context = TestParseContext::default();
    let mut signatures = Vec::new();
    for ty in [
        "bool", "int4", "int8", "float4", "float8", "text", "numeric", "bytea",
    ]
    .map(TypeReference::from_master_type_name)
    {
        context.setup_type(ty.clone());
        context.setup_type(ty.array_type());
        signatures.push(Signature::new(&[ty.clone(), ty], &PgBuiltInType::bool()));
    }
    let array = TypeReference::from_master_type_name("anyarray");
    signatures.push(Signature::new(
        &[array.clone(), array],
        &PgBuiltInType::bool(),
    ));
    (context, signatures)
}

pub(super) fn numeric_array_comparison(array_cast: bool) -> (TestParseContext, Vec<Signature>) {
    let (mut context, signatures) = comparison();
    let source = PgBuiltInType::int4();
    let target = PgBuiltInType::int8();
    let cast = crate::sem::CastDefinition::new(crate::sem::CastContext::implicit());
    context.set_get_implicit_cast_result(&source, &target, Some(cast.clone()));
    context.set_get_implicit_castable_result(
        &source,
        &std::collections::HashSet::from([source.clone(), target.clone()]),
    );
    if array_cast {
        context.set_get_implicit_cast_result(
            &source.array_type(),
            &target.array_type(),
            Some(cast),
        );
    }
    (context, signatures)
}
