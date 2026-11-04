use crate::AstAndContextPair;
use crate::ast::Dialect;
use crate::context::{BaseParseContext as _, ParseContext as _};
use crate::sem::SemAst;
use crate::test_helpers::query::catalog;

use crate::context::SchemaFileContext;
use crate::names::FullName;
use crate::parse_statements;
use crate::sem::{CastContext, CastDefinition, OverloadVariant, PgBuiltInType, TypeReference};
use crate::test_helpers::create_cache_context;

/// Explicit small composite fixtures; no builtin catalog or live database.
pub(crate) fn composite_query(sql: &str) -> AstAndContextPair<SchemaFileContext> {
    let mut cache_context = create_cache_context();
    let mut base = catalog();
    for name in ["singleton", "pair", "container"] {
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
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("array-query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    );
    for statement in parse_statements(
        "CREATE TYPE singleton AS (n integer); CREATE TYPE pair AS (n integer, m integer); CREATE TYPE container AS (item singleton)",
        Dialect::postgres(),
    ) {
        context = SemAst::from_ast(&statement.unwrap(), context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap(),
        context,
        &mut cache_context,
    )
    .unwrap()
}
