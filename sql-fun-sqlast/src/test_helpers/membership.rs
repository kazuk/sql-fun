pub(crate) use crate::test_helpers::query::expression;
use crate::test_helpers::statements::schema_postgres;
use crate::{
    ast::Dialect,
    context::SchemaFileContext,
    names::{FullName, OperatorName},
    sem::{
        BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition, OverloadVariant,
        PgBuiltInType, TypeReference,
    },
    test_helpers::{
        TestParseContext,
        scalar_catalog::{categorized_scalar_catalog, schema_from_base},
    },
};

/// Small catalog for membership binding and inference tests.
/// These tests need no builtin catalog loading or external services.
pub(crate) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

fn base_context(dialect: Dialect) -> TestParseContext {
    let mut base = categorized_scalar_catalog(dialect);
    let definitions = [
        PgBuiltInType::int4(),
        PgBuiltInType::text(),
        PgBuiltInType::bool(),
    ]
    .map(|ty| BinaryOperatorDefinition::new(&ty, &ty, &PgBuiltInType::bool(), true));
    for name in ["=", "<>"] {
        base.set_get_operator_op_result(
            &OperatorName::from_parts(None, name),
            true,
            true,
            Some(&OperatorDefinition::binary(&definitions)),
        );
    }
    base
}

pub(crate) fn context_for(dialect: Dialect) -> SchemaFileContext {
    from_base(base_context(dialect))
}

fn from_base(base: TestParseContext) -> SchemaFileContext {
    schema_from_base(base, std::path::Path::new("query.sql"))
}

pub(crate) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    schema_postgres(sql, context())
}

/// Concrete function inputs seed deferred composite-star expansion without a
/// full builtin catalog. The two row widths test the same semantic boundary.
pub(crate) fn composite_schema() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut base = base_context(Dialect::postgres());
    for name in ["singleton", "pair"] {
        let ty = TypeReference::concrete_type_ref(&FullName::with_schema("public", name), false);
        base.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        base.setup_function(
            &format!("take_{name}"),
            &[OverloadVariant::new(
                &Some(PgBuiltInType::int4()),
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
    schema_postgres(
        "CREATE TYPE singleton AS (n integer); CREATE TYPE pair AS (n integer, m integer)",
        from_base(base),
    )
}
