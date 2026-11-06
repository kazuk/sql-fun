//! Parser-built FROM fixtures converted before semantic analysis.
use crate::{
    ast::{DerivedTable, FromItem, Join, Relation, TableFunctions, TryFromWithTokens, XmlTable},
    names::FullName,
    sem::AnalysisError,
    syn::{Node, NodeInner, NodeList, WithClauseOpt},
    test_helpers::SynBuilder,
};
use ::sql_fun_core::IVec;

pub(crate) fn function(name: &str, alias: &str, rows: bool) -> Result<FromItem, AnalysisError> {
    let builder = SynBuilder::new();
    let call = builder.build_func_call_node(name);
    let item = if rows {
        call
    } else {
        builder.build_list_node(vec![call])
    };
    let source = builder.build_range_function(item, alias, rows);
    Ok(FromItem::functions(TableFunctions::try_from_with_tokens(
        &source,
        &IVec::default(),
    )?))
}

pub(crate) fn table_function(name: &str, alias: &str) -> Result<FromItem, AnalysisError> {
    let builder = SynBuilder::new();
    let source = builder.build_range_table_func(
        builder.build_func_call_node(name),
        builder.alias(alias, &[]),
    );
    Ok(FromItem::xml(XmlTable::try_from_with_tokens(
        &source,
        &IVec::default(),
    )?))
}

pub(crate) fn subselect() -> Result<FromItem, AnalysisError> {
    let builder = SynBuilder::new();
    let target = builder.res_target("name", NodeInner::AConst(builder.const_int4(10)).into());
    let query = builder.select_stmt(
        WithClauseOpt::none(),
        NodeList::from(Vec::<Node>::new()),
        vec![target],
    );
    let source = builder.range_subselect(builder.alias_name_only("alias1"), query);
    Ok(FromItem::query(DerivedTable::try_from_with_tokens(
        &source,
        &IVec::default(),
    )?))
}

pub(crate) fn inner_join() -> Result<FromItem, AnalysisError> {
    let builder = SynBuilder::new();
    let left = NodeInner::RangeVar(builder.range_var(
        &FullName::from_local_name("table1"),
        &builder.alias("t1", &[]).into(),
    ));
    let right = NodeInner::RangeVar(builder.range_var(
        &FullName::from_local_name("table2"),
        &builder.alias("t2", &[]).into(),
    ));
    let condition = builder.operator_equals(
        NodeInner::ColumnRef(builder.column_ref("t1", "id")).into(),
        NodeInner::ColumnRef(builder.column_ref("t2", "id")).into(),
    );
    let source = builder.inner_join(&left.into(), &right.into(), &condition);
    Ok(FromItem::join(Join::try_from_with_tokens(
        &source,
        &IVec::default(),
    )?))
}

pub(crate) fn relation(name: &str, alias: &str) -> Result<FromItem, AnalysisError> {
    let builder = SynBuilder::new();
    let source = builder.range_var(
        &FullName::from_local_name(name),
        &builder.alias(alias, &[]).into(),
    );
    Ok(FromItem::relation(Relation::try_from(&source)?))
}
