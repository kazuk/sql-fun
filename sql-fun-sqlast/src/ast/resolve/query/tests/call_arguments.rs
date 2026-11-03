//! Supplied call arguments survive analysis independently of function signatures.

use crate::{
    ast::{
        Dialect, StatementNode,
        resolve::query::test_helpers::{catalog, resolve},
    },
    context::ParseContext,
    names::AliasName,
    parse_statements,
    sem::{
        AnalysisError, DataSource, FuncCallExpr, FunctionParam, OverloadVariant, PgBuiltInType,
        QueryNameResolution, ScalarConstExpr as SemScalarConstExpr, SelectStatement, SemScalarExpr,
    },
};

fn projection(query: &SelectStatement) -> &SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}

fn call(query: &SelectStatement) -> &FuncCallExpr {
    projection(query)
        .as_function_call()
        .expect("preserved call")
}

#[rstest::rstest]
#[case::named("SELECT identity(arg => 1)", &[Some("arg")])]
#[case::old_notation("SELECT identity(arg := 1)", &[Some("arg")])]
#[case::reordered("SELECT identity(z => 1, a => 2)", &[Some("z"), Some("a")])]
#[case::mixed("SELECT identity(1, z => 2)", &[None, Some("z")])]
#[case::folded("SELECT identity(Arg => 1)", &[Some("arg")])]
#[case::quoted(r#"SELECT identity("Arg" => 1)"#, &[Some("Arg")])]
#[case::quoted_distinct(r#"SELECT identity(arg => 1, "Arg" => 2)"#, &[Some("arg"), Some("Arg")])]
fn retains_actuals_in_written_order(#[case] sql: &str, #[case] names: &[Option<&str>]) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let call = call(&query);
    assert_eq!(call.args().len(), names.len());
    assert_eq!(
        call.name_resolution(),
        &QueryNameResolution::function(&crate::names::FullName::with_schema("public", "identity"))
    );
    for (index, expected) in (0..names.len()).zip(names) {
        let argument = &call.args()[index];
        assert_eq!(argument.position(), index);
        assert_eq!(argument.supplied_name(), *expected);
        assert_eq!(
            argument.value(),
            &SemScalarExpr::literal(&crate::sem::ScalarConstExpr::new_integer(
                (index + 1) as i32
            ))
        );
        if expected.is_some() {
            assert!(argument.source_span().is_some());
            assert!(argument.arg_definition().is_none());
        }
    }
    if names.contains(&Some("arg")) || names.contains(&Some("z")) || names.contains(&Some("Arg")) {
        assert!(call.signature_is_deferred());
        assert!(call.overload().is_none());
        assert!(projection(&query).get_type().is_none());
        assert!(projection(&query).is_not_null().is_none());
    }
}

#[rstest::rstest]
#[case::known("identity", true)]
#[case::qualified("public.identity", true)]
#[case::missing("missing", false)]
#[case::wrong_schema("other.identity", false)]
fn checks_only_function_name(#[case] name: &str, #[case] found: bool) {
    let sql = format!("SELECT {name}(unknown_name => 'not an integer', extra => NULL)");
    let (query, context) = resolve(&sql, Dialect::postgres(), true).unwrap();
    let call = call(&query);
    assert_eq!(call.name_resolution().is_resolved(), found);
    assert_eq!(context.reported_problem_count(), usize::from(!found));
    assert_eq!(call.args().len(), 2);
    assert!(call.overload().is_none());
    assert!(call.signature_is_deferred());
    assert!(call.args()[0].arg_definition().is_none());
    assert!(call.args()[1].value().as_const().unwrap().is_null());
}

#[test]
fn does_not_select_overloads_or_insert_defaults() {
    let mut context = catalog();
    let default = SemScalarExpr::literal(&SemScalarConstExpr::new_integer(99));
    let overloads = [PgBuiltInType::int4(), PgBuiltInType::text()].map(|ty| {
        OverloadVariant::new(
            &Some(ty.clone()),
            &vec![
                FunctionParam::new_input_param(&Some("a".to_owned()), &Some(ty.clone()), &None),
                FunctionParam::new_input_param(
                    &Some("b".to_owned()),
                    &Some(ty),
                    &Some(default.clone()),
                ),
            ],
            true,
            true,
        )
    });
    context.setup_function("overloaded", &overloads);
    let statement = parse_statements("SELECT overloaded(a => 1)", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.node else {
        panic!("query")
    };
    let query = query.resolve_in(&mut context, None).unwrap();
    let call = call(&query);
    assert_eq!(context.reported_problem_count(), 0);
    assert!(call.name_resolution().is_resolved());
    assert_eq!(call.args().len(), 1);
    assert!(call.overload().is_none());
    assert!(call.args()[0].arg_definition().is_none());
}

#[rstest::rstest]
#[case::without_inference(false)]
#[case::with_inference(true)]
fn traversal_and_serialization_preserve_parameters(#[case] infer: bool) {
    let sql = "SELECT identity(c => $7, a => $2, again => $7, nested => identity(arg => $3))";
    let (mut query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    struct Parameters(Vec<u32>);
    impl crate::sem::traversal::SemanticVisitorMut for Parameters {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let SemScalarExpr::Param(value) = expression {
                self.0.push(value.position());
            }
            Ok(())
        }
    }
    let mut parameters = Parameters(Vec::new());
    query.walk(&mut parameters).unwrap();
    assert_eq!(parameters.0, [7, 2, 7, 3]);
    let original = ::serde_json::to_value(projection(&query)).unwrap();
    let mut restored: SemScalarExpr = ::serde_json::from_value(original.clone()).unwrap();
    assert_eq!(::serde_json::to_value(&restored).unwrap(), original);
    let mut parameters = Parameters(Vec::new());
    restored.walk(&mut parameters).unwrap();
    assert_eq!(parameters.0, [7, 2, 7, 3]);
    assert_eq!(
        restored.as_function_call().unwrap().args()[0].supplied_name(),
        Some("c")
    );
    assert_eq!(restored.as_function_call().unwrap().args()[1].position(), 1);
    assert!(restored.as_function_call().unwrap().signature_is_deferred());
    if infer {
        for position in [7, 2, 3] {
            assert!(
                context
                    .parameter_constraints()
                    .parameter(position)
                    .unwrap()
                    .constraint()
                    .is_any()
            );
        }
    }
}

#[rstest::rstest]
#[case::positional("SELECT identity(VARIADIC ARRAY[1,2])", None)]
#[case::named("SELECT identity(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
fn preserves_explicit_variadic(#[case] sql: &str, #[case] name: Option<&str>) {
    let (query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let call = call(&query);
    assert!(call.modifiers().variadic());
    assert!(call.signature_is_deferred());
    assert!(call.overload().is_none());
    assert_eq!(call.args().len(), 1);
    assert_eq!(call.args()[0].supplied_name(), name);
    let restored: SemScalarExpr =
        ::serde_json::from_str(&::serde_json::to_string(projection(&query)).unwrap()).unwrap();
    assert!(restored.as_function_call().unwrap().modifiers().variadic());
}

#[rstest::rstest]
#[case::found("identity", true)]
#[case::missing("missing", false)]
fn retains_table_call_without_return_signature(#[case] name: &str, #[case] found: bool) {
    let (mut query, context) = resolve(
        &format!("SELECT 1 FROM {name}(a => $4) AS f"),
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), usize::from(!found));
    let source = query
        .query_clauses()
        .unwrap()
        .body()
        .source_scope()
        .unwrap()
        .resolve_alias(&AliasName::from("f"))
        .unwrap()
        .0;
    let function = source.deferred_function().unwrap();
    assert_eq!(function.call().name_resolution().is_resolved(), found);
    assert_eq!(function.call().args()[0].supplied_name(), Some("a"));
    let serialized = ::serde_json::to_value(&query).unwrap();
    let restored: SelectStatement = ::serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(::serde_json::to_value(&restored).unwrap(), serialized);
    struct Parameters(usize);
    impl crate::sem::traversal::SemanticVisitorMut for Parameters {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let SemScalarExpr::Param(value) = expression {
                assert_eq!(value.position(), 4);
                self.0 += 1;
            }
            Ok(())
        }
    }
    let mut parameters = Parameters(0);
    query.walk(&mut parameters).unwrap();
    assert_eq!(parameters.0, 1);
}

#[test]
fn retains_rows_from_arguments_and_modifiers() {
    let (query, context) = resolve(
        "SELECT 1 FROM ROWS FROM(identity(VARIADIC xs => ARRAY[1])) AS f",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let source = query
        .query_clauses()
        .unwrap()
        .body()
        .source_scope()
        .unwrap()
        .resolve_alias(&AliasName::from("f"))
        .unwrap()
        .0;
    let DataSource::RowFunctions(functions) = source else {
        panic!("ROWS FROM")
    };
    let call = functions.functions()[0].deferred_function().unwrap();
    assert!(call.name_resolution().is_resolved());
    assert!(call.modifiers().variadic());
    assert_eq!(call.args()[0].supplied_name(), Some("xs"));
}

#[rstest::rstest]
#[case::duplicate("SELECT identity(a => 1, a => 2)", "call_argument_duplicate_name")]
#[case::folded_duplicate("SELECT identity(A => 1, a => 2)", "call_argument_duplicate_name")]
#[case::positional_after_named(
    "SELECT identity(a => 1, 2)",
    "call_argument_positional_after_named"
)]
fn diagnoses_signature_independent_call_shape(#[case] sql: &str, #[case] code: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(call(&query).args().len(), 2);
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(context.reported_problems()[0].code(), code);
}

#[test]
fn retains_named_value_properties_and_source_span() {
    let sql = r#"SELECT identity("Arg" => 1, n => NULL)"#;
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let argument = &call(&query).args()[0];
    assert_eq!(
        argument.source_span().unwrap().get_from(sql),
        Some(r#""Arg""#)
    );
    assert_eq!(argument.arg_expr().get_type(), argument.value().get_type());
    assert_eq!(
        argument.arg_expr().is_not_null(),
        argument.value().is_not_null()
    );
    assert_eq!(call(&query).args()[1].arg_expr().is_not_null(), Some(false));
    let restored: SemScalarExpr =
        ::serde_json::from_value(::serde_json::to_value(projection(&query)).unwrap()).unwrap();
    assert_eq!(
        restored.as_function_call().unwrap().args()[0]
            .source_span()
            .unwrap()
            .get_from(sql),
        Some(r#""Arg""#)
    );
}

#[test]
fn grouping_matches_named_calls_at_different_source_locations() {
    let (_, context) = resolve(
        "SELECT identity(a => n) FROM (SELECT 1 AS n) s GROUP BY identity(a => n)",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
}

#[rstest::rstest]
#[case::unqualified("SELECT value FROM identity(arg => 1)")]
#[case::qualified("SELECT identity.value FROM identity(arg => 1)")]
#[case::aliased("SELECT value FROM identity(arg => 1) AS f")]
fn retains_unknown_columns_from_deferred_functions(#[case] sql: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    assert!(matches!(
        projection(&query),
        SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedFunction(_))
    ));
    assert!(projection(&query).get_type().is_none());
}

#[test]
fn unknown_function_columns_remain_ambiguous_between_sources() {
    let (query, context) = resolve(
        "SELECT value FROM identity(arg => 1) AS f, identity(arg => 2) AS g",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    assert!(matches!(
        projection(&query),
        SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_))
    ));
}

#[test]
fn deferred_projection_cannot_establish_view_row_preservation() {
    let (query, context) =
        resolve("SELECT identity(arg => 1)", Dialect::postgres(), false).unwrap();
    assert!(!projection(&query).preserves_view_rows(&context));
}

#[rstest::rstest]
#[case::duplicate("SELECT identity(a => 1, a => 2)", "a")]
#[case::quoted_duplicate(r#"SELECT identity("Arg" => 1, "Arg" => 2)"#, r#""Arg""#)]
#[case::parameter("SELECT identity(a => 1, $2)", "$2")]
fn call_diagnostics_identify_the_offending_argument(#[case] sql: &str, #[case] token: &str) {
    let (_, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    let problem = &context.reported_problems()[0];
    let span = problem.statement_relative_span().unwrap();
    assert_eq!(
        sql.get(span.offset()..span.offset() + span.len()),
        Some(token)
    );
    assert_eq!(span.offset(), sql.rfind(token).unwrap());
}

#[test]
fn unanchored_positional_literals_do_not_get_fabricated_diagnostic_spans() {
    let (_, context) = resolve("SELECT identity(a => 1, 2)", Dialect::postgres(), false).unwrap();
    assert!(
        context.reported_problems()[0]
            .statement_relative_span()
            .is_none()
    );
}
