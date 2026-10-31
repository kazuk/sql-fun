use crate::context::{BaseParseContext, ParseContext, SchemaFileContext};

/// Explicit small composite fixtures; no builtin catalog or live database.
pub(super) fn composite_query(sql: &str) -> crate::AstAndContextPair<SchemaFileContext> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let mut base = super::super::super::test_helpers::catalog();
    for name in ["singleton", "pair", "container"] {
        let ty = crate::sem::TypeReference::concrete_type_ref(
            &crate::sem::FullName::with_schema("public", name),
            false,
        );
        base.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(crate::sem::CastDefinition::new(
                crate::sem::CastContext::no_conversion(),
            )),
        );
        base.setup_function(
            &format!("take_{name}"),
            &[crate::sem::OverloadVariant::new(
                &Some(crate::sem::PgBuiltInType::int4()),
                &vec![crate::sem::FunctionParam::new_input_param(
                    &None,
                    &Some(ty),
                    &None,
                )],
                false,
                false,
            )],
        );
    }
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("array-query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    );
    for statement in crate::parse_statements(
        "CREATE TYPE singleton AS (n integer); CREATE TYPE pair AS (n integer, m integer); CREATE TYPE container AS (item singleton)",
        crate::ast::Dialect::postgres(),
    ) {
        context = statement
            .unwrap()
            .analyze(context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    crate::parse_statements(sql, crate::ast::Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .analyze(context, &mut cache_context)
        .unwrap()
}
