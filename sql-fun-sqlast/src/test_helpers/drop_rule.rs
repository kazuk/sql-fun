use crate::AstAndContextPair;
use crate::ast::{Dialect, Statement};
use crate::context::BaseParseContext as _;
use crate::parse_statements;
use crate::sem::AnalysisError;
use crate::test_helpers::statements;

use crate::context::{FrozenBaseContext, SchemaFileContext};
use crate::names::{FullName, TableName, ViewName};
use crate::sem::RuleIdentity;
pub(crate) const CREATE_AUDIT: &str =
    "CREATE RULE audit AS ON INSERT TO item_view DO INSTEAD NOTHING";
pub(crate) fn identity() -> RuleIdentity {
    RuleIdentity::new(&FullName::with_schema("public", "item_view"), "audit")
}

pub(crate) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
    statements::layer(base, std::path::Path::new("drop-rule.sql"))
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
pub(crate) fn table_rule_count(context: &FrozenBaseContext) -> usize {
    let target = FullName::with_schema("public", "items");
    context
        .get_table(&TableName::from(target.clone()))
        .unwrap()
        .rules()
        .resolve(context)
        .len()
}
pub(crate) fn active_count(context: &FrozenBaseContext) -> usize {
    let target = FullName::with_schema("public", "item_view");
    context
        .get_view(&ViewName::from_full_name(&target))
        .unwrap()
        .rules()
        .unwrap()
        .resolve(context)
        .len()
}
