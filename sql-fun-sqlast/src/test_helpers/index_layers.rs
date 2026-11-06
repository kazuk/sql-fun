use crate::ast::{IndexExpression, ddl::Index};
use crate::context::ParseContext as _;
use crate::sem::AnalysisError;
use crate::test_helpers::schema_layer;

use crate::context::SchemaFileContext;
use crate::names::{FullName, SchemaName, TableName};
use crate::sem::{CreateIndex, IndexElementCollection};
use crate::test_helpers::TestParseContext;
use std::path::Path;

pub(crate) fn index(schema: &str, table: &str, name: &str) -> CreateIndex {
    CreateIndex::new(
        &FullName::with_schema(schema, name),
        &TableName::from(FullName::with_schema(schema, table)),
        &IndexElementCollection::default(),
    )
}

pub(crate) fn layer(
    path: &[&str],
    indexes: &[CreateIndex],
) -> Result<SchemaFileContext, AnalysisError> {
    let base = TestParseContext::default().freeze();
    let mut context = schema_layer(Path::new("indexes.sql"), &base);
    let mut schemas = Vec::new();
    for name in path {
        schemas.push(SchemaName::from(name));
    }
    context.set_search_path(&schemas);
    for index in indexes {
        context = context.apply_create_index(index)?;
    }
    Ok(context)
}

/// Parser-built index fixtures, converted before semantic analysis.
pub(crate) fn aliased_index_keys() -> Result<Vec<IndexExpression>, AnalysisError> {
    use crate::{ast::TryFromWithTokens, syn::SortByDir, test_helpers::SynBuilder};
    let builder = SynBuilder::new();
    [
        builder.index_elem_column("id", SortByDir::SortbyDesc.into(), Some("id_alias")),
        builder.index_elem_column("name", SortByDir::SortbyDefault.into(), None),
    ]
    .iter()
    .map(|key| IndexExpression::try_from_with_tokens(key, &::sql_fun_core::IVec::default()))
    .collect()
}

pub(crate) fn expression_index_key() -> Result<Vec<IndexExpression>, AnalysisError> {
    use crate::{
        ast::TryFromWithTokens,
        syn::{Node, NodeInner, SortByDir},
        test_helpers::SynBuilder,
    };
    let builder = SynBuilder::new();
    let expression = Node::from(NodeInner::AConst(builder.const_int4(42)));
    let key = builder.index_elem_expr(expression, SortByDir::SortbyDefault.into(), None);
    Ok(vec![IndexExpression::try_from_with_tokens(
        &key,
        &::sql_fun_core::IVec::default(),
    )?])
}

pub(crate) fn index_statement() -> Result<Index, AnalysisError> {
    use crate::{
        ast::TryFromWithTokens,
        syn::{AliasOpt, RangeVarOpt, SortByDir},
        test_helpers::SynBuilder,
    };
    let builder = SynBuilder::new();
    let relation = RangeVarOpt::from(
        builder.range_var(&FullName::with_schema("public", "items"), &AliasOpt::none()),
    );
    let keys = builder.index_elem_list(vec![
        builder.index_elem_column("id", SortByDir::SortbyDesc.into(), None),
        builder.index_elem_column("name", SortByDir::SortbyDefault.into(), None),
    ]);
    let included = builder.index_elem_list(vec![builder.index_elem_column(
        "metadata",
        SortByDir::SortbyDefault.into(),
        None,
    )]);
    let syntax = builder.index_stmt("idx_items", relation, keys, included, false);
    Index::try_from_with_tokens(&syntax, &::sql_fun_core::IVec::default())
}
