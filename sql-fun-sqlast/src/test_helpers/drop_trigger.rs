use crate::AstAndContextPair;
use crate::ast::{Dialect, Statement};
use crate::context::BaseParseContext as _;
use crate::parse_statements;
use crate::sem::AnalysisError;
use crate::test_helpers::statements;

use crate::context::{FrozenBaseContext, SchemaFileContext};
use crate::names::{FullName, TableName, ViewName};
use crate::sem::TriggerIdentity;
pub(crate) const CREATE_AUDIT: &str =
    "CREATE TRIGGER audit INSTEAD OF INSERT ON item_view FOR EACH ROW EXECUTE FUNCTION audit_fn()";
pub(crate) fn identity() -> TriggerIdentity {
    TriggerIdentity::postgres(
        &FullName::with_schema("public", "item_view"),
        &FullName::with_schema("public", "audit"),
    )
}

pub(crate) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
    statements::layer(base, std::path::Path::new("drop-trigger.sql"))
}
pub(crate) fn statement(sql: &str) -> Statement {
    parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .expect("valid test SQL")
}
pub(crate) fn apply(
    context: SchemaFileContext,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    statements::apply(context, sql, Dialect::postgres())
}
pub(crate) fn table_trigger_count(context: &FrozenBaseContext) -> usize {
    let target = FullName::with_schema("public", "items");
    context
        .get_table(&TableName::from(target.clone()))
        .unwrap()
        .triggers()
        .resolve(context, &target)
        .len()
}
pub(crate) fn active_count(context: &FrozenBaseContext) -> usize {
    let target = FullName::with_schema("public", "item_view");
    context
        .get_view(&ViewName::from_full_name(&target))
        .unwrap()
        .triggers()
        .unwrap()
        .resolve(context, &target)
        .len()
}
