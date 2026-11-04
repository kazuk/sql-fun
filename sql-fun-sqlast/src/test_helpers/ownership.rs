//! Explicit catalog fixtures for ownership semantics.

use crate::sem::convert;
use crate::{
    ast::{StatementNode, TryFromWithTokens},
    context::{MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::SchemaName,
    sem::{AnalysisError, AstAndContextPair, OwnedObjectDefinition, PgBuiltInType, Role},
    test_helpers::{SynBuilder, TestParseContext, create_cache_context, schema_layer, source_span},
};

pub(crate) fn ownership_context() -> SchemaFileContext {
    let mut inventory = TestParseContext::default();
    inventory.set_search_path(&[SchemaName::from("public")]);
    for role in ["alice", "bob", "Quoted Owner"] {
        inventory.insert_role(&Role::from(role));
    }
    for reference in [
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::text(),
    ] {
        inventory.setup_type(reference);
    }
    let mut context = schema_layer(std::path::Path::new("ownership.sql"), &inventory.freeze());
    context.set_source_path_with_span(&source_span());
    context
}

pub(crate) fn ownership_statement(sql: &str) -> Result<StatementNode, AnalysisError> {
    StatementNode::try_from_with_tokens(
        &SynBuilder::new().parse(sql),
        &::sql_fun_core::IVec::default(),
    )
}
pub(crate) fn apply_ownership_sql<C: ParseContext>(
    context: C,
    sql: &str,
) -> Result<AstAndContextPair<C>, AnalysisError> {
    convert(&ownership_statement(sql)?).resolve_and_apply(context, &mut create_cache_context())
}

pub(crate) fn known_owner_count(definitions: &[OwnedObjectDefinition]) -> usize {
    let mut count = 0;
    for definition in definitions {
        if definition.owner().is_some() {
            count += 1;
        }
    }
    count
}
