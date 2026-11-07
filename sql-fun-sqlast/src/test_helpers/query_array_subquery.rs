use crate::{
    AstAndContextPair,
    ast::Dialect,
    context::{BaseParseContext as _, ParseContext as _, SchemaFileContext},
    names::FullName,
    parse_statements,
    sem::{CastContext, CastDefinition, OverloadVariant, PgBuiltInType, SemAst, TypeReference},
    test_helpers::{TestParseContext, create_cache_context, query::catalog},
};

/// Explicit small composite fixtures; no builtin catalog or live database.
pub(crate) fn composite_query(sql: &str) -> AstAndContextPair<SchemaFileContext> {
    composite_query_with_catalog(sql, catalog())
}

pub(crate) fn composite_query_with_catalog(
    sql: &str,
    mut base: TestParseContext,
) -> AstAndContextPair<SchemaFileContext> {
    let mut cache_context = create_cache_context();
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

use crate::sem::AnalysisProblem;

pub(crate) fn reports_width_problem(problems: &[AnalysisProblem]) -> bool {
    for problem in problems {
        if problem.code() == "array_subquery_columns" {
            return true;
        }
    }
    false
}
