use crate::ast::Dialect;
use crate::sem::{
    AnalysisError, CatalogNameLookup, FullName, PgBuiltInType, SelectStatement, TypeDefinition,
};
use crate::test_helpers::TestParseContext;

/// Explicit bindings and three type facts; no real catalog initialization.
pub(crate) fn context(dialect: Dialect) -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(dialect);
    for (ty, collatable) in [
        (PgBuiltInType::text(), true),
        (PgBuiltInType::int4(), false),
        (PgBuiltInType::bool(), false),
    ] {
        context.setup_type(ty.clone());
        context.set_get_type_impl_result(
            ty.full_name(),
            &TypeDefinition::builtin_with_category(ty.full_name(), None, Some(collatable)),
        );
    }
    for local in ["C", "POSIX"] {
        let name = FullName::from_local_name(local);
        let resolved = FullName::with_schema("pg_catalog", local);
        context.set_collation_lookup(&name, &CatalogNameLookup::found(&name, &resolved));
        context.set_collation_lookup(&resolved, &CatalogNameLookup::found(&resolved, &resolved));
    }
    context
}

pub(crate) fn query(
    sql: &str,
    context: &mut TestParseContext,
) -> Result<SelectStatement, Box<dyn std::error::Error>> {
    use crate::context::ParseContext;
    let statement = crate::parse_statements(sql, context.dialect())
        .next()
        .expect("one query")?;
    let analyzed = statement.analyze(std::mem::take(context))?;
    let query = analyzed
        .ast()
        .clone()
        .into_select_statemtnt()
        .ok_or_else(|| AnalysisError::unexpected_input_error("expected SELECT"))?;
    *context = analyzed.into_context();
    Ok(query)
}

pub(crate) fn expression(query: &SelectStatement) -> &crate::sem::SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}
