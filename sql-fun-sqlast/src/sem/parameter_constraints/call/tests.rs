mod test_helpers;

use super::super::{ConstraintGraph, InputTypeConstraint, ParameterConstraints};
use crate::sem::{PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

#[test]
fn defaults_are_bounded_without_restricting_explicit_assignments() {
    let (mut context, signatures) = test_helpers::comparison();
    context.forbid_type_enumeration();
    let mut graph = ConstraintGraph::default();
    let left = graph.parameter(1);
    let right = graph.parameter(2);
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(&context, &[left, right, result], &signatures, "comparison");
    graph.solve().unwrap();
    assert_eq!(
        graph.domain(left),
        &InputTypeConstraint::types(
            [
                "bool", "int4", "int8", "float4", "float8", "text", "numeric"
            ]
            .map(TypeReference::from_master_type_name)
        )
    );
    assert_eq!(graph.domain(right), graph.domain(left));
    let constraints = ParameterConstraints::from_graph(&graph);
    drop(context);
    let bytea = TypeReference::from_master_type_name("bytea");
    assert!(constraints.allows(&BTreeMap::from([(1, bytea.clone()), (2, bytea)])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::bool()),
        (2, PgBuiltInType::int4())
    ])));
    let custom_array: TypeReference = "public.custom[]".parse().unwrap();
    assert!(constraints.allows(&BTreeMap::from([
        (1, custom_array.clone()),
        (2, custom_array)
    ])));
}

#[rstest::rstest]
fn later_array_information_survives_earlier_default_inference(
    #[values(false, true)] intermediate_solve: bool,
) {
    let (context, signatures) = test_helpers::comparison();
    let mut graph = ConstraintGraph::default();
    let left = graph.parameter(1);
    let right = graph.parameter(2);
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(&context, &[left, right, result], &signatures, "comparison");
    if intermediate_solve {
        graph.solve().unwrap();
    }
    let array = PgBuiltInType::int4().array_type();
    graph.restrict(left, &InputTypeConstraint::exact(&array));
    graph.solve().unwrap();
    assert_eq!(graph.domain(right).single_type(), Some(array.clone()));
    let constraints = ParameterConstraints::from_graph(&graph);
    assert!(constraints.allows(&BTreeMap::from([(2, array)])));
    assert!(!constraints.allows(&BTreeMap::from([(2, PgBuiltInType::text())])));
}

#[test]
fn unrelated_catalog_growth_does_not_instantiate_array_comparisons() {
    let (mut context, signatures) = test_helpers::comparison();
    context.forbid_type_enumeration();
    for index in 0..50 {
        context.setup_type(format!("public.unrelated_{index}").parse().unwrap());
    }
    let facts = super::CallFacts::prepare(
        &context,
        &signatures,
        &[InputTypeConstraint::any(), InputTypeConstraint::any()],
    );
    let defaults = InputTypeConstraint::types(super::default_types());
    let instantiated = facts.signatures(&[defaults.clone(), defaults]);
    assert_eq!(instantiated.len(), 7);
    assert!(instantiated.iter().all(|entry| {
        entry
            .signature
            .inputs()
            .iter()
            .all(|ty| ty.is_array() != Some(true))
    }));
}

#[rstest::rstest]
fn array_only_unknown_inputs_remain_unresolved_and_inspectable(
    #[values(false, true)] scalar_alternative: bool,
) {
    let (context, signatures) = test_helpers::comparison();
    let mut graph = ConstraintGraph::default();
    let left = graph.parameter(1);
    let right = graph.parameter(2);
    let result = graph.node(&InputTypeConstraint::any());
    graph.restrict(result, &InputTypeConstraint::exact(&PgBuiltInType::bool()));
    let mut array_signatures = signatures[8..].to_vec();
    if scalar_alternative {
        array_signatures.push(crate::sem::parameter_resolution::Signature::new(
            &[PgBuiltInType::int4(), PgBuiltInType::int4()],
            &PgBuiltInType::int4(),
        ));
    }
    graph.call(
        &context,
        &[left, right, result],
        &array_signatures,
        "array comparison",
    );
    graph.solve().unwrap();
    assert!(graph.domain(left).is_any());
    assert_eq!(
        graph.domain(result).single_type(),
        Some(PgBuiltInType::bool())
    );
    let constraints = ParameterConstraints::from_graph(&graph);
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
}

#[test]
fn concrete_array_declaration_precedes_polymorphic_instantiation() {
    let (context, mut signatures) = test_helpers::comparison();
    let array = PgBuiltInType::int4().array_type();
    signatures.push(crate::sem::parameter_resolution::Signature::new(
        &[array.clone(), array.clone()],
        &PgBuiltInType::int4(),
    ));
    let mut graph = ConstraintGraph::default();
    let left = graph.parameter(1);
    let right = graph.parameter(2);
    graph.restrict(left, &InputTypeConstraint::exact(&array));
    graph.restrict(right, &InputTypeConstraint::exact(&array));
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(
        &context,
        &[left, right, result],
        &signatures,
        "custom comparison",
    );
    graph.solve().unwrap();
    assert_eq!(
        graph.domain(result).single_type(),
        Some(PgBuiltInType::int4())
    );
}

#[rstest::rstest]
fn explicit_array_assignments_use_retained_array_casts(#[values(false, true)] array_cast: bool) {
    let (context, signatures) = test_helpers::numeric_array_comparison(array_cast);
    let mut graph = ConstraintGraph::default();
    let left = graph.parameter(1);
    let right = graph.parameter(2);
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(&context, &[left, right, result], &signatures, "comparison");
    graph.solve().unwrap();
    let constraints = ParameterConstraints::from_graph(&graph);
    drop(context);
    assert_eq!(
        constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4().array_type()),
            (2, PgBuiltInType::int8().array_type()),
        ])),
        array_cast
    );
}

#[test]
fn result_context_keeps_non_default_input_types_before_defaulting() {
    let (mut context, _) = test_helpers::comparison();
    let date = TypeReference::from_master_type_name("date");
    context.setup_type(date.clone());
    let signatures = [
        crate::sem::parameter_resolution::Signature::new(std::slice::from_ref(&date), &date),
        crate::sem::parameter_resolution::Signature::new(
            &[PgBuiltInType::int4()],
            &PgBuiltInType::int4(),
        ),
    ];
    let mut graph = ConstraintGraph::default();
    let input = graph.parameter(1);
    let result = graph.node(&InputTypeConstraint::exact(&date));
    graph.call(
        &context,
        &[input, result],
        &signatures,
        "result-constrained call",
    );
    graph.solve().unwrap();
    assert_eq!(graph.domain(input).single_type(), Some(date.clone()));
    let constraints = ParameterConstraints::from_graph(&graph);
    assert!(constraints.allows(&BTreeMap::from([(1, date)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
}

#[test]
fn unknown_expression_results_report_unsupported_inference_without_invalid_sql() {
    let (mut context, signatures) = test_helpers::comparison();
    context.forbid_type_enumeration();
    let mut graph = ConstraintGraph::default();
    let left = graph.node(&InputTypeConstraint::any());
    let right = graph.node(&InputTypeConstraint::any());
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(&context, &[left, right, result], &signatures, "comparison");
    graph.solve().unwrap();
    graph.report_inference_problems(&mut context).unwrap();
    assert_eq!(context.reported_problems().len(), 1);
    assert!(context.has_reported_problem("inference_candidate_space_unsupported"));
    assert!(graph.domain(left).is_any());
    assert_eq!(
        graph.domain(result).single_type(),
        Some(PgBuiltInType::bool())
    );
}

#[test]
fn sql_context_resolves_expression_results_before_the_unsupported_boundary() {
    let (mut context, signatures) = test_helpers::comparison();
    context.forbid_type_enumeration();
    let mut graph = ConstraintGraph::default();
    let left = graph.node(&InputTypeConstraint::any());
    let right = graph.node(&InputTypeConstraint::any());
    let result = graph.node(&InputTypeConstraint::any());
    graph.call(&context, &[left, right, result], &signatures, "comparison");
    let custom: TypeReference = "public.custom[]".parse().unwrap();
    graph.restrict(left, &InputTypeConstraint::exact(&custom));
    graph.prepare_projections(&context);
    graph.solve().unwrap();
    graph.report_inference_problems(&mut context).unwrap();
    assert!(context.reported_problems().is_empty());
    assert_eq!(graph.domain(right).single_type(), Some(custom));
}
