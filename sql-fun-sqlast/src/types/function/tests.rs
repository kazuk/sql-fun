use crate::sem::{ColumnName, FullName};
use crate::types::{
    Candidates, DefaultExpression, FunctionOverloads, FunctionParameter, FunctionResult,
    FunctionSignature, Never, ParameterMode, PolymorphicCategory, PolymorphicPosition,
    PolymorphicRole, SignaturePolymorphism, TypeDefinition, base::BaseType,
};

mod test_helpers {
    use super::{
        BaseType, ColumnName, FullName, FunctionOverloads, FunctionParameter, FunctionResult,
        FunctionSignature, ParameterMode,
    };
    pub(super) fn integer() -> BaseType {
        BaseType::new(&FullName::with_schema("pg_catalog", "int4"))
    }
    pub(super) fn text() -> BaseType {
        BaseType::new(&FullName::with_schema("pg_catalog", "text"))
    }
    pub(super) fn correlated() -> FunctionOverloads {
        let integer = integer();
        let text = text();
        let a = FunctionParameter::new(
            &Some(ColumnName::new("a")),
            ParameterMode::input(),
            &integer,
            &None,
        );
        let b = FunctionParameter::new(
            &Some(ColumnName::new("b")),
            ParameterMode::input(),
            &text,
            &None,
        );
        let name = FullName::with_schema("public", "f");
        FunctionOverloads::new(&[
            FunctionSignature::new(
                &name,
                &[a.clone(), b.clone()],
                &FunctionResult::scalar(&text),
                false,
                false,
            ),
            FunctionSignature::new(
                &name,
                &[b, a],
                &FunctionResult::scalar(&integer),
                false,
                false,
            ),
        ])
    }
}

#[test]
fn successive_projection_and_filtering_preserve_whole_signatures() {
    let overloads = test_helpers::correlated();
    let integers = test_helpers::integer();
    let initial = overloads.argument_candidates(0);
    assert_eq!(
        initial
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .candidates()
            .len(),
        2
    );
    let filtered = overloads.filter_argument(0, &integers);
    assert_eq!(filtered.signatures(), &overloads.signatures()[..1]);
    assert_eq!(
        filtered
            .result_candidates()
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .candidates()
            .len(),
        1
    );
    assert!(filtered.filter_result(&integers).is_empty());
    assert!(
        overloads
            .filter_result(&integers)
            .filter_argument(0, &integers)
            .is_empty()
    );
    assert_eq!(filtered.filter_argument(1, &test_helpers::text()), filtered);
    assert!(filtered.filter_argument(1, &integers).is_empty());
}

#[test]
fn named_filters_resolve_each_signature_independently() {
    let overloads = test_helpers::correlated();
    let a = ColumnName::new("a");
    // a is int in both signatures, but occupies a different position in each.
    assert_eq!(
        overloads
            .filter_argument_by_name(&a, &test_helpers::integer())
            .unwrap(),
        overloads
    );
    let filtered = overloads.filter_result(&test_helpers::text());
    assert_eq!(
        filtered
            .filter_argument_by_name(&a, &test_helpers::integer())
            .unwrap(),
        filtered
    );
    assert!(
        filtered
            .filter_argument_by_name(&ColumnName::new("b"), &test_helpers::integer())
            .unwrap()
            .is_empty()
    );
    assert!(
        filtered
            .argument_candidates_by_name(&ColumnName::new("absent"))
            .unwrap()
            .as_any()
            .is::<Never>()
    );
    assert!(
        filtered
            .filter_argument_by_name(&ColumnName::new("A"), &test_helpers::integer())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn empty_missing_and_duplicate_candidate_contracts() {
    let overloads = test_helpers::correlated();
    let empty = overloads.filter_argument(0, &Never::new());
    assert!(empty.is_empty());
    assert!(empty.candidates().as_any().is::<Never>());
    assert!(empty.argument_candidates(0).as_any().is::<Never>());
    assert!(empty.result_candidates().as_any().is::<Never>());
    assert!(overloads.argument_candidates(99).as_any().is::<Never>());
    assert!(
        overloads
            .filter_argument(99, &test_helpers::integer())
            .is_empty()
    );
    let first = overloads.signatures()[0].clone();
    let duplicate = FunctionOverloads::new(&[first.clone(), first.clone()]);
    assert_eq!(duplicate, FunctionOverloads::new(&[first]));
    assert_eq!(
        duplicate
            .filter_argument(0, &test_helpers::integer())
            .signatures()
            .len(),
        2
    );
    let reversed = FunctionOverloads::new(&[
        overloads.signatures()[1].clone(),
        overloads.signatures()[0].clone(),
    ]);
    assert_eq!(overloads, reversed);
}

#[test]
fn callable_positions_exclude_outputs_and_names_are_exact() {
    let integer = test_helpers::integer();
    let p = |name: &str, mode| {
        FunctionParameter::new(&Some(ColumnName::new(name)), mode, &integer, &None)
    };
    let signature = FunctionSignature::new(
        &FullName::with_schema("public", "f"),
        &[
            p("out", ParameterMode::output()),
            p("Case", ParameterMode::in_out()),
            p("table", ParameterMode::table()),
            p("rest", ParameterMode::variadic()),
        ],
        &FunctionResult::procedure(),
        true,
        false,
    );
    assert_eq!(
        signature.argument(0).unwrap().name(),
        Some(&ColumnName::new("Case"))
    );
    assert_eq!(
        signature.argument(1).unwrap().mode(),
        ParameterMode::variadic()
    );
    assert!(signature.argument(2).is_none());
    assert!(
        signature
            .argument_by_name(&ColumnName::new("out"))
            .unwrap()
            .is_none()
    );
    assert!(
        signature
            .argument_by_name(&ColumnName::new("case"))
            .unwrap()
            .is_none()
    );
    assert!(
        signature
            .argument_by_name(&ColumnName::new("Case"))
            .unwrap()
            .is_some()
    );
    assert!(
        FunctionOverloads::new(&[signature])
            .result_candidates()
            .as_any()
            .is::<Never>()
    );
}

#[test]
fn ambiguous_names_are_errors_without_selecting_a_parameter() {
    let integer = test_helpers::integer();
    let p = FunctionParameter::new(
        &Some(ColumnName::new("a")),
        ParameterMode::input(),
        &integer,
        &None,
    );
    let s = FunctionSignature::new(
        &FullName::with_schema("public", "f"),
        &[p.clone(), p],
        &FunctionResult::scalar(&integer),
        false,
        false,
    );
    let overloads = FunctionOverloads::new(&[s]);
    assert!(
        overloads
            .argument_candidates_by_name(&ColumnName::new("a"))
            .is_err()
    );
    assert!(
        overloads
            .filter_argument_by_name(&ColumnName::new("a"), &integer)
            .is_err()
    );
}

#[test]
fn signature_identity_preserves_metadata_and_polymorphic_links() {
    let positions = [
        PolymorphicPosition::new(PolymorphicCategory::anyelement(), PolymorphicRole::input()),
        PolymorphicPosition::new(PolymorphicCategory::anyelement(), PolymorphicRole::output()),
    ];
    let scope = SignaturePolymorphism::new(&positions).unwrap();
    let default = Some(DefaultExpression::sql_list("1, ROW(2, 3)", 1));
    let parameter = FunctionParameter::new(
        &Some(ColumnName::new("a")),
        ParameterMode::input(),
        &scope.types()[0],
        &default,
    );
    let name = FullName::with_schema("public", "f");
    let s = FunctionSignature::new(
        &name,
        std::slice::from_ref(&parameter),
        &FunctionResult::scalar(&scope.types()[1]),
        true,
        true,
    );
    assert!(s.contains_polymorphic());
    assert!(s.same_type(s.clone_box().as_ref()));
    assert_ne!(
        s,
        FunctionSignature::new(
            &name,
            std::slice::from_ref(&parameter),
            &FunctionResult::set(&scope.types()[1]),
            true,
            true
        )
    );
    assert_ne!(
        s,
        FunctionSignature::new(
            &name,
            std::slice::from_ref(&parameter),
            s.result(),
            false,
            true
        )
    );
    assert_ne!(
        s,
        FunctionSignature::new(&name, &[parameter], s.result(), true, false)
    );
    assert_eq!(s.parameters()[0].default_expression().unwrap().index(), 1);
    let independent = SignaturePolymorphism::new(&positions).unwrap();
    assert!(
        FunctionOverloads::new(&[s])
            .filter_result(&independent.types()[1])
            .is_empty()
    );
}

#[test]
fn named_filter_can_distinguish_overloads_with_reordered_and_different_named_types() {
    let integer = test_helpers::integer();
    let text = test_helpers::text();
    let name = FullName::with_schema("public", "f");
    let first = FunctionSignature::new(
        &name,
        &[
            FunctionParameter::new(
                &Some(ColumnName::new("a")),
                ParameterMode::input(),
                &integer,
                &None,
            ),
            FunctionParameter::new(
                &Some(ColumnName::new("b")),
                ParameterMode::input(),
                &text,
                &None,
            ),
        ],
        &FunctionResult::scalar(&text),
        false,
        false,
    );
    let second = FunctionSignature::new(
        &name,
        &[
            FunctionParameter::new(
                &Some(ColumnName::new("b")),
                ParameterMode::input(),
                &integer,
                &None,
            ),
            FunctionParameter::new(
                &Some(ColumnName::new("a")),
                ParameterMode::input(),
                &text,
                &None,
            ),
        ],
        &FunctionResult::scalar(&integer),
        false,
        false,
    );
    let overloads = FunctionOverloads::new(&[first.clone(), second]);
    let filtered = overloads
        .filter_argument_by_name(&ColumnName::new("a"), &integer)
        .unwrap();
    assert_eq!(filtered.signatures(), &[first]);
    assert!(filtered.filter_result(&integer).is_empty());
    assert_eq!(
        filtered
            .filter_argument_by_name(&ColumnName::new("b"), &text)
            .unwrap(),
        filtered
    );
}
