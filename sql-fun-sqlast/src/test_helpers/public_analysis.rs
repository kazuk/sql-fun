use crate::AstAndContextPair;
use crate::ast::{Dialect, Statement};
use crate::names::{FullName, OperatorName};
use crate::parse_statements;
use crate::sem::SemAst;
use crate::sem::{
    AnalysisError, BinaryOperatorDefinition, CastContext, CastDefinition,
    FunctionOverloadCollection, FunctionParam, OperatorDefinition, OverloadVariant, PgBuiltInType,
};
use crate::test_helpers::{TestParseContext, create_cache_context};

pub(crate) fn statement(sql: &str, dialect: Dialect) -> Statement {
    parse_statements(sql, dialect).next().unwrap().unwrap()
}

pub(crate) fn context() -> TestParseContext {
    let mut context = TestParseContext::default();
    let int = PgBuiltInType::int4();
    let small = PgBuiltInType::int2();
    let text = PgBuiltInType::text();
    context.setup_type(int.clone());
    context.setup_type(small.clone());
    context.setup_type(text.clone());
    context.setup_type(PgBuiltInType::bool());
    for ty in [&int, &small, &text] {
        context.set_get_implicit_cast_result(
            ty,
            ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
    }
    context.set_get_implicit_cast_result(
        &small,
        &int,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let add = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(&int, &int, &int, true)]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "+"),
        true,
        true,
        Some(&add),
    );
    let lower = OverloadVariant::new(
        &Some(text.clone()),
        &vec![FunctionParam::new_input_param(&None, &Some(text), &None)],
        true,
        false,
    );
    context.set_get_function_impl_result(
        &FullName::with_schema("pg_catalog", "lower"),
        &FunctionOverloadCollection::new(&[lower]),
    );
    context
}

pub(crate) fn analyze(
    sql: &str,
    dialect: Dialect,
) -> Result<AstAndContextPair<TestParseContext>, AnalysisError> {
    let mut cache_context = create_cache_context();
    SemAst::from_ast(
        &parse_statements(sql, dialect).next().unwrap().unwrap(),
        context().with_dialect(dialect),
        &mut cache_context,
    )
}
