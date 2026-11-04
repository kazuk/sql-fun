//! Supplied syntax and candidate-specific formal identities survive resolution.

use crate::{
    context::ParseContext,
    names::{AliasName, FullName},
    sem::TypeReference,
    sem::{
        AnalysisError, PgBuiltInType, QueryNameResolution, ScalarConstExpr, SelectStatement,
        SemScalarExpr,
    },
    test_helpers::{
        call_arguments::{call, resolve},
        query::expression,
    },
};

#[rstest::rstest]
#[case::reordered("SELECT f(c => 9, a => 1)", &["c", "a"], &[9, 1])]
#[case::old_notation("SELECT f(c := 9, a := 1)", &["c", "a"], &[9, 1])]
#[case::mixed("SELECT f(1, c => 9)", &["a", "c"], &[1, 9])]
#[case::folded("SELECT f(A => 1)", &["a"], &[1])]
fn binds_written_arguments_without_inserting_defaults(
    #[case] sql: &str,
    #[case] formals: &[&str],
    #[case] values: &[i32],
) {
    let (query, context) = resolve(sql, false);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let call = call(&query);
    assert!(!call.signature_is_deferred());
    assert!(call.overload().is_some());
    assert_eq!(call.args().len(), formals.len());
    assert_eq!(
        call.name_resolution(),
        &QueryNameResolution::function(&FullName::with_schema("public", "f"))
    );
    for (position, formal) in formals.iter().enumerate() {
        let argument = &call.args()[position];
        assert_eq!(argument.position(), position);
        assert_eq!(
            argument
                .arg_definition()
                .as_ref()
                .unwrap()
                .name()
                .map(String::as_str),
            Some(*formal)
        );
        assert_eq!(
            argument.value(),
            &SemScalarExpr::literal(&ScalarConstExpr::new_integer(values[position]))
        );
        if argument.supplied_name().is_some() {
            assert!(argument.source_span().is_some());
        }
    }
    let original = ::serde_json::to_value(expression(&query)).unwrap();
    let restored: SemScalarExpr = ::serde_json::from_value(original.clone()).unwrap();
    assert_eq!(::serde_json::to_value(restored).unwrap(), original);
    // Omitted defaults do not prove non-nullness for a strict function.
    assert_eq!(expression(&query).is_not_null(), Some(false));
}

#[rstest::rstest]
#[case::duplicate("SELECT f(a => 1, a => 2)", "call_argument_duplicate_name")]
#[case::folded_duplicate("SELECT f(A => 1, a => 2)", "call_argument_duplicate_name")]
#[case::placement("SELECT f(a => 1, 2)", "call_argument_positional_after_named")]
#[case::unknown_name("SELECT f(z => 1)", "call_argument_unknown_name")]
#[case::quoted_name(r#"SELECT f("A" => 1)"#, "call_argument_unknown_name")]
#[case::assigned_twice("SELECT f(1, a => 2)", "call_argument_assigned_twice")]
#[case::missing_required("SELECT f(c => 1)", "call_argument_missing_required")]
#[case::wrong_type("SELECT f(a => TRUE)", "function_overload_resolution_failed")]
#[case::missing_function("SELECT missing(a => 1)", "function_not_found")]
fn failed_binding_retains_arguments_and_lookup(#[case] sql: &str, #[case] code: &str) {
    let (query, context) = resolve(sql, false);
    assert_eq!(
        context.reported_problem_count(),
        1,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(context.reported_problems()[0].code(), code);
    let call = call(&query);
    assert!(call.overload().is_none());
    assert!(!call.args().is_empty());
    assert_eq!(
        call.name_resolution().is_resolved(),
        code != "function_not_found"
    );
}

#[rstest::rstest]
#[case::positional("SELECT v(VARIADIC ARRAY[1,2])", None)]
#[case::named("SELECT v(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
#[case::ordinary_array("SELECT arr(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
fn explicit_variadic_binds_an_array_identity(#[case] sql: &str, #[case] name: Option<&str>) {
    let (query, context) = resolve(sql, true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let call = call(&query);
    assert!(call.modifiers().variadic());
    assert!(call.overload().is_some());
    assert_eq!(call.args().len(), 1);
    assert_eq!(call.args()[0].supplied_name(), name);
    assert_eq!(
        call.args()[0].arg_definition().as_ref().unwrap().get_type(),
        Some(&PgBuiltInType::int4().array_type())
    );
}

#[test]
fn inference_preserves_reordered_positions_and_formal_associations() {
    let (mut query, context) = resolve(
        "SELECT f(c => $7::integer, a => $2::integer, b => $7::integer)",
        true,
    );
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    for position in [7, 2] {
        assert_eq!(
            context
                .parameter_constraints()
                .parameter(position)
                .unwrap()
                .constraint()
                .admissible_types(),
            Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
        );
    }
    let SemScalarExpr::Constrained(constrained) = expression(&query) else {
        panic!("constrained call")
    };
    let binding = constrained.function_binding().unwrap();
    assert!(binding.overload().is_some());
    assert_eq!(
        binding
            .argument_definition(0)
            .unwrap()
            .name()
            .map(String::as_str),
        Some("c")
    );
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
    assert_eq!(parameters.0, [7, 2, 7]);
    let serialized = ::serde_json::to_value(&query).unwrap();
    let restored: SelectStatement = ::serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(::serde_json::to_value(restored).unwrap(), serialized);
}

#[rstest::rstest]
#[case::scalar("SELECT f(a => $1) FROM f(a => 1) AS t")]
#[case::table("SELECT t.* FROM f(a => $1::integer) AS t")]
#[case::rows_from("SELECT t.* FROM ROWS FROM(f(a => $1)) AS t")]
fn table_consumers_receive_call_constraints(#[case] sql: &str) {
    let (query, context) = resolve(sql, true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .admissible_types(),
        Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
    );
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
    let source = query
        .query_clauses()
        .unwrap()
        .body()
        .source_scope()
        .unwrap()
        .resolve_alias(&AliasName::from("t"))
        .unwrap()
        .0;
    if let Some(function) = source.deferred_function() {
        assert!(function.call().overload().is_some());
    }
}

#[rstest::rstest]
#[case::scalar("SELECT poly(value => $1), f(a => $1)")]
#[case::table("SELECT t.* FROM poly(value => $1) AS t, f(a => $1) AS other")]
fn deferred_polymorphism_uses_other_occurrences(#[case] sql: &str) {
    let (query, context) = resolve(sql, true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(expression(&query).get_type(), None);
    assert_eq!(
        context
            .parameter_constraints()
            .result_column(0)
            .unwrap()
            .admissible_types(),
        Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
    );
}

#[rstest::rstest]
#[case::aggregate("SELECT named_sum(x=>1)", false)]
#[case::window("SELECT named_sum(x=>1) OVER ()", true)]
fn aggregate_notation_has_window_specific_legality(#[case] sql: &str, #[case] valid: bool) {
    // An explicit catalog isolates call legality from CREATE AGGREGATE parsing.
    let (query, context) = resolve(sql, false);
    assert_eq!(context.reported_problem_count(), usize::from(!valid));
    assert_eq!(call(&query).overload().is_some(), valid);
    if !valid {
        assert_eq!(
            context.reported_problems()[0].code(),
            "function_overload_resolution_failed"
        );
    }
}

#[test]
fn named_polymorphism_retains_nondefault_assignments_and_correlations() {
    let (query, context) = resolve("SELECT same(b => $7, a => $2)", true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(expression(&query).get_type().is_none());
    let constraints = context.parameter_constraints();
    let bytea = TypeReference::from_master_type_name("bytea");
    assert!(constraints.allows(&std::collections::BTreeMap::from([
        (7, bytea.clone()),
        (2, bytea.clone())
    ])));
    assert!(!constraints.allows(&std::collections::BTreeMap::from([
        (7, bytea),
        (2, PgBuiltInType::int4())
    ])));
}

#[test]
fn deferred_polymorphism_uses_a_nondefault_type_from_a_later_occurrence() {
    let (query, context) = resolve("SELECT poly(value=>$1), bytes(value=>$1)", true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        expression(&query).get_type(),
        Some(TypeReference::from_master_type_name("bytea"))
    );
}

#[test]
fn deferred_out_columns_receive_argument_types() {
    let (query, context) = resolve(
        "SELECT t.* FROM polyout(value=>$1) AS t, bytes(value=>$1) AS other",
        true,
    );
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        expression(&query).get_type(),
        Some(TypeReference::from_master_type_name("bytea"))
    );
}

#[test]
fn deferred_variadic_array_keeps_element_and_array_identity_distinct() {
    let (query, context) = resolve("SELECT polyarray(VARIADIC xs=>$1), arr(xs=>$1)", true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
}
