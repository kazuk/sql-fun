use crate::ast::{Dialect, Statement};
use crate::context::ParseContext;
use crate::sem::{
    ColumnDefinition, ColumnName, FunctionParam, OverloadVariant, PgBuiltInType, SchemaName,
    TypeReference, WildcardExpansion,
};
use crate::test_helpers::{TestParseContext, source_span};

pub(super) fn statement(sql: &str) -> Statement {
    crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
}

pub(super) fn catalog() -> TestParseContext {
    let mut context = TestParseContext::default();
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    context.setup_type(PgBuiltInType::int4());
    let record = TypeReference::from_master_type_name("record");
    context.setup_type(record.clone());
    context.setup_function(
        "record_arg",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![FunctionParam::new_input_param(&None, &Some(record), &None)],
            false,
            false,
        )],
    );
    for (name, names) in [
        ("t", &["id", "name"][..]),
        ("u", &["id", "value"][..]),
        ("empty", &[][..]),
    ] {
        let columns = names
            .iter()
            .copied()
            .map(ColumnDefinition::wildcard_int4_column)
            .collect::<Vec<_>>();
        context.setup_table(name, &columns, &source_span());
    }
    context
}

impl ColumnDefinition {
    fn wildcard_int4_column(name: &str) -> Self {
        Self::new(
            &Some(ColumnName::from(name)),
            Some(&PgBuiltInType::int4()),
            Some(false),
            &[],
        )
    }
}

pub(super) fn expansions(statement: &Statement) -> Vec<WildcardExpansion> {
    let mut cache_context = crate::CacheContext::new();
    let pair = statement.analyze(catalog(), &mut cache_context).unwrap();
    assert!(
        pair.context().reported_problems().is_empty(),
        "{:?}",
        pair.context().reported_problems()
    );
    pair.context()
        .analysis_state()
        .wildcard_expansions()
        .to_vec()
}

pub(super) fn rewritten(sql: &str) -> (Statement, usize) {
    let mut statement = statement(sql);
    let groups = expansions(&statement);
    let count = statement.rewrite_wildcards(&groups).unwrap();
    (statement, count)
}

pub(super) fn rows_catalog() -> TestParseContext {
    let mut context = catalog();
    context.setup_type(PgBuiltInType::int8());
    let table = crate::sem::FullName::with_schema("public", "t");
    context.set_get_type_impl_result(
        &table,
        &crate::sem::TypeDefinition::table_ref(&crate::sem::TableName::from(table.clone())),
    );
    context.setup_function(
        "rows_fun",
        &[OverloadVariant::new(
            &Some(TypeReference::from_full_name(table)),
            &vec![],
            false,
            false,
        )],
    );
    context.set_get_function_impl_result(
        &crate::sem::FullName::with_schema("pg_catalog", "row_number"),
        &crate::sem::FunctionOverloadCollection::new(&[OverloadVariant::new(
            &Some(PgBuiltInType::int8()),
            &vec![],
            false,
            false,
        )]),
    );
    context.setup_type(PgBuiltInType::bool());
    context.set_get_operator_op_result(
        &crate::sem::OperatorName::from_parts(Some("pg_catalog"), ">="),
        true,
        true,
        Some(&crate::sem::OperatorDefinition::binary(&[
            crate::sem::BinaryOperatorDefinition::new(
                &PgBuiltInType::int8(),
                &PgBuiltInType::int4(),
                &PgBuiltInType::bool(),
                true,
            ),
        ])),
    );
    context.set_get_function_impl_result(
        &crate::sem::FullName::with_schema("pg_catalog", "count"),
        &crate::sem::FunctionOverloadCollection::new(&[OverloadVariant::new(
            &Some(PgBuiltInType::int8()),
            &vec![],
            false,
            true,
        )]),
    );
    context
}

/// Compare the observable projection contract without exposing collection walks.
pub(super) trait ResultShapeAssertions {
    fn assert_same_result_shape(&self, other: &Self, sql: &str);
}
impl ResultShapeAssertions for crate::sem::ResultColumnCollection {
    fn assert_same_result_shape(&self, other: &Self, sql: &str) {
        assert_eq!(self.len(), other.len(), "{sql}");
        for ordinal in 0..self.len() {
            let left = self.get_column_at(ordinal).unwrap();
            let right = other.get_column_at(ordinal).unwrap();
            assert_eq!(left.column_name(), right.column_name(), "{sql}");
            assert_eq!(left.col_type(), right.col_type(), "{sql}");
        }
    }
}
