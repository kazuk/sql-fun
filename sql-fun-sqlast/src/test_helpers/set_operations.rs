use crate::{
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    sem::{AnalysisError, AstAndContextPair, SchemaName},
    test_helpers::common_types,
};

/// Minimal common-type catalog, retaining its real definitions for later assignments.
pub(crate) fn context(dialect: Dialect) -> SchemaFileContext {
    let mut base = common_types::context().with_dialect(dialect);
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("set.sql"),
        &base.freeze(),
        &args,
        &metadata,
    );
    context.set_source_path_with_span(&crate::test_helpers::source_span());
    context
}

/// Analyze through the public statement boundary without loading builtin catalogs.
pub(crate) fn analyze(sql: &str) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .analyze(
            context(Dialect::postgres()),
            &mut crate::test_helpers::create_cache_context(),
        )
}

/// A local two-column record for deferred projection/width contracts.
pub(crate) fn composite_context() -> Result<SchemaFileContext, AnalysisError> {
    let record = crate::test_helpers::ownership::apply_ownership_sql(
        context(Dialect::postgres()),
        "CREATE TYPE pair AS (n integer, m integer)",
    )?;
    Ok(crate::test_helpers::ownership::apply_ownership_sql(
        record.into_context(),
        "CREATE FUNCTION take_pair(pair) RETURNS integer LANGUAGE SQL AS 'SELECT 1'",
    )?
    .into_context())
}
