use crate::sem::{CastContext, CastDefinition, PgBuiltInType, TypeCategory, TypeDefinition};
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
