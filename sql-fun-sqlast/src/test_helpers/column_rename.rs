//! Lightweight schema-chain fixtures for column rename contracts.
use crate::{
    context::{
        BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext,
    },
    sem::{AnalysisError, FullName, SchemaName},
    test_helpers::{TestParseContext, ownership::apply_ownership_sql, source_span},
};
pub(crate) fn schema(statements: &[&str]) -> Result<SchemaFileContext, AnalysisError> {
    let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"postgres", "dialect":"postgres", "version":"17", "search_path":["public"]})).unwrap();
    let mut base = TestParseContext::default().with_metadata(&metadata);
    base.set_search_path(&[SchemaName::from("public")]);
    base.setup_type(crate::sem::PgBuiltInType::int4());
    base.setup_type(crate::sem::TypeReference::from_master_type_name("trigger"));
    let frozen = base.freeze();
    let mut context = child(&frozen);
    for sql in statements {
        context = apply_ownership_sql(context, sql)?.into_context();
    }
    assert!(
        context.problems().is_empty(),
        "fixture diagnostics: {:?}",
        context.problems()
    );
    Ok(context)
}
pub(crate) fn name(local: &str) -> FullName {
    FullName::with_schema("public", local)
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_schema(statements: &[&str]) -> Result<SchemaFileContext, AnalysisError> {
    let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"SQLite", "dialect":"SQLite", "version":"3", "search_path":["main"]})).unwrap();
    let mut base = TestParseContext::default()
        .with_metadata(&metadata)
        .with_dialect(crate::ast::Dialect::sqlite());
    base.set_search_path(&[SchemaName::from("main")]);
    let mut context = child(&base.freeze());
    for sql in statements {
        context = sqlite(context, sql)?.into_context();
    }
    assert!(
        context.problems().is_empty(),
        "fixture diagnostics: {:?}",
        context.problems()
    );
    Ok(context)
}
#[cfg(feature = "sqlite")]
pub(crate) fn sqlite(
    context: SchemaFileContext,
    sql: &str,
) -> Result<crate::AstAndContextPair<SchemaFileContext>, AnalysisError> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    crate::parse_statements(sql, crate::ast::Dialect::sqlite())
        .next()
        .unwrap()
        .expect("valid SQLite syntax")
        .analyze(context, &mut cache_context)
}

pub(crate) fn child(base: &FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("column-rename.sql"),
        base,
        base.get_args(),
        base.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

/// Observe semantic column bindings without depending on expression nesting.
pub(crate) fn bound_columns(expression: &crate::sem::SemScalarExpr) -> Vec<String> {
    let mut visitor = BoundColumns::default();
    expression.clone().walk(&mut visitor).unwrap();
    visitor.0
}
#[derive(Default)]
struct BoundColumns(Vec<String>);
impl crate::sem::traversal::SemanticVisitor for BoundColumns {
    fn expression(
        &mut self,
        expression: &mut crate::sem::SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        if let Some((_, column, _)) = expression.namespace_reference() {
            self.0.push(column.as_str().to_owned());
        }
        Ok(())
    }
}

/// Inspect retained whole-row field names through the semantic composite API.
pub(crate) fn whole_row_columns(
    expression: &crate::sem::SemScalarExpr,
    context: &impl ParseContext,
) -> Vec<Vec<crate::sem::ColumnName>> {
    let mut visitor = WholeRowColumns {
        context,
        columns: Vec::new(),
    };
    expression.clone().walk(&mut visitor).unwrap();
    visitor.columns
}
struct WholeRowColumns<'a, C> {
    context: &'a C,
    columns: Vec<Vec<crate::sem::ColumnName>>,
}
impl<C: ParseContext> crate::sem::traversal::SemanticVisitor for WholeRowColumns<'_, C> {
    fn expression(
        &mut self,
        expression: &mut crate::sem::SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        if expression.as_whole_row().is_some() {
            self.columns
                .push(expression.composite_columns(self.context).unwrap().names());
        }
        Ok(())
    }
}
