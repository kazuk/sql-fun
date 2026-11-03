//! Small owned catalog and public analysis comparisons for cache contracts.

use crate::{
    AstAndContextPair, CacheContext,
    ast::Dialect,
    context::{FrozenBaseContext, ParseContext, SchemaFileContext},
    names::SchemaName,
    parse_statements,
    sem::PgBuiltInType,
    test_helpers::{TestParseContext, domain_casts, schema_layer},
};
use std::collections::BTreeMap;

pub(crate) fn base() -> FrozenBaseContext {
    let mut context = TestParseContext::default();
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        context.setup_type(ty.clone());
        context.setup_type(ty.array_type());
    }
    for operator in ["=", "<>", ">=", "<=", ">", "<"] {
        context.setup_binary(operator, "int4", "int4", "bool", true);
    }
    context.setup_binary("+", "int4", "int4", "int4", true);
    let mut context = schema_layer(std::path::Path::new("cache.sql"), &context.freeze());
    domain_casts::domain(&mut context, "amount", Some(&PgBuiltInType::int4()));
    context.freeze()
}

pub(crate) fn analyze(
    sql: &str,
    base: &FrozenBaseContext,
    cache: &mut CacheContext,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let context = schema_layer(std::path::Path::new("query.sql"), base);
    Ok(parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, cache)?)
}

pub(crate) fn assert_equivalent(
    expected: &AstAndContextPair<SchemaFileContext>,
    actual: &AstAndContextPair<SchemaFileContext>,
) {
    assert_eq!(
        ::serde_json::to_value(expected.context().problems()).unwrap(),
        ::serde_json::to_value(actual.context().problems()).unwrap(),
    );
    let expected_query = expected.ast().clone().into_select_statemtnt().unwrap();
    let actual_query = actual.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        ::serde_json::to_value(expected_query.result_columns()).unwrap(),
        ::serde_json::to_value(actual_query.result_columns()).unwrap(),
    );
    let expected = expected.context().parameter_constraints();
    let actual = actual.context().parameter_constraints();
    for position in 1..=2 {
        assert_eq!(expected.parameter(position), actual.parameter(position));
    }
    for index in 0..expected_query.result_columns().len() {
        assert_eq!(expected.result_column(index), actual.result_column(index));
    }
    // Check joint assignments, not merely each parameter's independent domain.
    for left in [
        PgBuiltInType::int4(),
        PgBuiltInType::bool(),
        PgBuiltInType::int4().array_type(),
    ] {
        let single = BTreeMap::from([(1, left.clone())]);
        assert_eq!(expected.allows(&single), actual.allows(&single));
        for right in [
            PgBuiltInType::int4(),
            PgBuiltInType::bool(),
            PgBuiltInType::int4().array_type(),
        ] {
            let pair = BTreeMap::from([(1, left.clone()), (2, right)]);
            assert_eq!(expected.allows(&pair), actual.allows(&pair));
        }
    }
}
