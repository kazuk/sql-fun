use crate::{
    ast::{Dialect, Statement},
    context::ParseContext as _,
    names::{ColumnName, FullName, OperatorName, SchemaName, TableName},
    parse_statements,
    sem::{
        BinaryOperatorDefinition, ColumnDefinition, FunctionOverloadCollection, FunctionParam,
        OperatorDefinition, OverloadVariant, PgBuiltInType, ResultColumnCollection, SemAst,
        TypeDefinition, TypeReference, WildcardExpansion,
    },
    test_helpers::{TestParseContext, create_cache_context, source_span},
};

pub(crate) fn statement(sql: &str) -> Statement {
    parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
}

pub(crate) fn catalog() -> TestParseContext {
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
            .map(wildcard_int4_column)
            .collect::<Vec<_>>();
        context.setup_table(name, &columns, &source_span());
    }
    context
}

fn wildcard_int4_column(name: &str) -> ColumnDefinition {
    ColumnDefinition::new(
        &Some(ColumnName::from(name)),
        Some(&PgBuiltInType::int4()),
        Some(false),
        &[],
    )
}

pub(crate) fn expansions(statement: &Statement) -> Vec<WildcardExpansion> {
    let mut cache_context = create_cache_context();
    let pair = SemAst::from_ast(statement, catalog(), &mut cache_context).unwrap();
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

pub(crate) fn rewritten(sql: &str) -> (Statement, usize) {
    let mut statement = statement(sql);
    let groups = expansions(&statement);
    let count = WildcardExpansion::rewrite(&groups, &mut statement).unwrap();
    (statement, count)
}

pub(crate) fn rows_catalog() -> TestParseContext {
    let mut context = catalog();
    context.setup_type(PgBuiltInType::int8());
    let table = FullName::with_schema("public", "t");
    context.set_get_type_impl_result(
        &table,
        &TypeDefinition::table_ref(&TableName::from(table.clone())),
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
        &FullName::with_schema("pg_catalog", "row_number"),
        &FunctionOverloadCollection::new(&[OverloadVariant::new(
            &Some(PgBuiltInType::int8()),
            &vec![],
            false,
            false,
        )]),
    );
    context.setup_type(PgBuiltInType::bool());
    context.set_get_operator_op_result(
        &OperatorName::from_parts(Some("pg_catalog"), ">="),
        true,
        true,
        Some(&OperatorDefinition::binary(&[
            BinaryOperatorDefinition::new(
                &PgBuiltInType::int8(),
                &PgBuiltInType::int4(),
                &PgBuiltInType::bool(),
                true,
            ),
        ])),
    );
    context.set_get_function_impl_result(
        &FullName::with_schema("pg_catalog", "count"),
        &FunctionOverloadCollection::new(&[OverloadVariant::new(
            &Some(PgBuiltInType::int8()),
            &vec![],
            false,
            true,
        )]),
    );
    context
}

/// Compare the observable projection contract without exposing collection walks.
pub(crate) trait ResultShapeAssertions {
    fn assert_same_result_shape(&self, other: &Self, sql: &str);
}
impl ResultShapeAssertions for ResultColumnCollection {
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
