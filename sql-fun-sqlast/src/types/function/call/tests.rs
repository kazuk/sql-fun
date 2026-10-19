use crate::sem::{ColumnName, FullName};
use crate::types::{
    CallArgument, Candidates, DefaultExpression, FunctionOverloads, FunctionParameter,
    FunctionResult, FunctionSignature, Never, ParameterMode, TypeDefinition, array::ArrayType,
    base::BaseType,
};

mod test_helpers {
    use super::{
        BaseType, ColumnName, DefaultExpression, FullName, FunctionParameter, FunctionResult,
        FunctionSignature, ParameterMode, TypeDefinition,
    };

    pub(super) fn integer() -> BaseType {
        BaseType::new(&FullName::with_schema("pg_catalog", "int4"))
    }
    pub(super) fn text() -> BaseType {
        BaseType::new(&FullName::with_schema("pg_catalog", "text"))
    }
    pub(super) fn parameter(
        name: &str,
        type_: &dyn TypeDefinition,
        mode: ParameterMode,
        default: bool,
    ) -> FunctionParameter {
        let expression = if default {
            Some(DefaultExpression::sql_list("NULL", 0))
        } else {
            None
        };
        FunctionParameter::new(&Some(ColumnName::new(name)), mode, type_, &expression)
    }
    pub(super) fn signature(
        parameters: &[FunctionParameter],
        result: &dyn TypeDefinition,
    ) -> FunctionSignature {
        FunctionSignature::new(
            &FullName::with_schema("public", "f"),
            parameters,
            &FunctionResult::scalar(result),
            false,
            false,
        )
    }
}

#[rstest::rstest]
#[case::one(1, 1)]
#[case::two(2, 1)]
#[case::missing(0, 0)]
#[case::excess(3, 0)]
fn call_arity_filters_whole_signatures(#[case] count: usize, #[case] expected: usize) {
    let integer = test_helpers::integer();
    let input = test_helpers::parameter("a", &integer, ParameterMode::input(), false);
    let second = test_helpers::parameter("b", &integer, ParameterMode::input(), false);
    let signatures = [
        test_helpers::signature(std::slice::from_ref(&input), &integer),
        test_helpers::signature(&[input, second], &test_helpers::text()),
    ];
    let overloads = FunctionOverloads::new(&signatures);
    let arguments = vec![CallArgument::positional(&integer); count];
    let matches = overloads.match_call(&arguments).unwrap();
    assert_eq!(matches.overloads().signatures().len(), expected);
    if expected > 0 {
        assert_eq!(
            matches.overloads().signatures(),
            &signatures[count - 1..count]
        );
    } else {
        assert!(matches.result_candidates().as_any().is::<Never>());
    }
}

#[test]
fn reordered_names_project_syntax_positions_and_preserve_result_correlation() {
    let integer = test_helpers::integer();
    let text = test_helpers::text();
    let a = test_helpers::parameter("a", &integer, ParameterMode::input(), false);
    let b = test_helpers::parameter("b", &text, ParameterMode::input(), false);
    let first = test_helpers::signature(&[a.clone(), b.clone()], &integer);
    let second = test_helpers::signature(&[b, a], &text);
    let overloads = FunctionOverloads::new(&[first.clone(), second.clone(), first]);
    let matches = overloads
        .match_call(&[
            CallArgument::named(&ColumnName::new("b"), &text),
            CallArgument::named(&ColumnName::new("a"), &integer),
        ])
        .unwrap();
    assert_eq!(matches.overloads(), overloads);
    assert_eq!(matches.overloads().signatures().len(), 3);
    assert!(
        matches
            .argument_candidates(0)
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .contains(&text)
    );
    assert!(
        matches
            .argument_candidates(1)
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .contains(&integer)
    );
    assert!(matches.argument_candidates(2).as_any().is::<Never>());
    let filtered = matches.filter_result(&text);
    assert_eq!(filtered.overloads().signatures(), &[second]);
    assert!(
        filtered
            .argument_candidates(0)
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .contains(&text)
    );
}

#[rstest::rstest]
#[case::default_omitted(0, false, true)]
#[case::mixed(1, false, true)]
#[case::duplicate_assignment(1, true, false)]
#[case::unknown_name(0, true, false)]
fn defaults_and_mixed_calls(
    #[case] positional: usize,
    #[case] invalid: bool,
    #[case] expected: bool,
) {
    let integer = test_helpers::integer();
    let name = if invalid { "a" } else { "b" };
    let parameters = [
        test_helpers::parameter("a", &integer, ParameterMode::in_out(), false),
        test_helpers::parameter("output", &integer, ParameterMode::output(), false),
        test_helpers::parameter("b", &integer, ParameterMode::input(), true),
        test_helpers::parameter("column", &integer, ParameterMode::table(), false),
    ];
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&parameters, &integer)]);
    let mut args = vec![CallArgument::positional(&integer); positional];
    if positional == 0 && !invalid {
        args.push(CallArgument::named(&ColumnName::new("a"), &integer));
    } else {
        let name = if positional == 0 { "missing" } else { name };
        args.push(CallArgument::named(&ColumnName::new(name), &integer));
    }
    assert_eq!(!overloads.match_call(&args).unwrap().is_empty(), expected);
}

#[rstest::rstest]
#[case::expanded(0, true)]
#[case::array(1, true)]
#[case::empty_call(2, false)]
#[case::named_array(3, true)]
#[case::named_without_variadic(4, false)]
#[case::scalar_explicit(5, false)]
fn variadic_calls_project_element_or_array(#[case] form: u8, #[case] expected: bool) {
    let integer = test_helpers::integer();
    let array = ArrayType::new(&integer);
    let parameter = test_helpers::parameter("xs", &array, ParameterMode::variadic(), false);
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&[parameter], &integer)]);
    let args = match form {
        0 => vec![
            CallArgument::positional(&integer),
            CallArgument::positional(&integer),
        ],
        1 => vec![CallArgument::positional(&array).with_variadic()],
        2 => vec![],
        3 => vec![CallArgument::named(&ColumnName::new("xs"), &array).with_variadic()],
        4 => vec![CallArgument::named(&ColumnName::new("xs"), &array)],
        _ => vec![CallArgument::positional(&integer).with_variadic()],
    };
    let matches = overloads.match_call(&args).unwrap();
    assert_eq!(!matches.is_empty(), expected);
    if expected {
        let projected = matches.argument_candidates(0);
        let candidates = projected.as_any().downcast_ref::<Candidates>().unwrap();
        if form == 0 {
            assert!(candidates.contains(&integer));
        } else {
            assert!(candidates.contains(&array));
        }
    }
}

#[test]
fn supplied_type_candidates_and_never_constrain_each_value() {
    let integer = test_helpers::integer();
    let text = test_helpers::text();
    let parameter = test_helpers::parameter("a", &integer, ParameterMode::input(), false);
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&[parameter], &text)]);
    let candidates = Candidates::from_candidates(&[integer.clone_box(), text.clone_box()]);
    let matches = overloads
        .match_call(&[CallArgument::positional(candidates.as_ref())])
        .unwrap();
    assert!(!matches.is_empty());
    assert!(
        matches
            .result_candidates()
            .as_any()
            .downcast_ref::<Candidates>()
            .unwrap()
            .contains(&text)
    );
    assert!(
        overloads
            .match_call(&[CallArgument::positional(&Never::new())])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_order_and_duplicate_names_do_not_choose_a_signature() {
    let integer = test_helpers::integer();
    let a = test_helpers::parameter("a", &integer, ParameterMode::input(), false);
    let b = test_helpers::parameter("b", &integer, ParameterMode::input(), false);
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&[a.clone(), b], &integer)]);
    let named = CallArgument::named(&ColumnName::new("a"), &integer);
    assert!(
        overloads
            .match_call(&[named.clone(), CallArgument::positional(&integer)])
            .unwrap()
            .is_empty()
    );
    assert!(
        overloads
            .match_call(&[named.clone(), named.clone()])
            .unwrap()
            .is_empty()
    );
    let ambiguous = FunctionOverloads::new(&[test_helpers::signature(&[a.clone(), a], &integer)]);
    assert!(ambiguous.match_call(&[named]).is_err());
}

#[test]
fn zero_inputs_and_variadic_defaults_allow_omission() {
    let integer = test_helpers::integer();
    let zero = test_helpers::signature(&[], &integer);
    let parameter = test_helpers::parameter(
        "xs",
        &ArrayType::new(&integer),
        ParameterMode::variadic(),
        true,
    );
    let defaulted = test_helpers::signature(&[parameter], &integer);
    let overloads = FunctionOverloads::new(&[zero, defaulted]);
    assert_eq!(overloads.match_call(&[]).unwrap().overloads(), overloads);
}

#[test]
fn quoted_names_are_exact_and_explicit_variadic_must_be_last() {
    let integer = test_helpers::integer();
    let parameter = test_helpers::parameter("Exact", &integer, ParameterMode::input(), false);
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&[parameter], &integer)]);
    assert!(
        !overloads
            .match_call(&[CallArgument::named(&ColumnName::new("Exact"), &integer)])
            .unwrap()
            .is_empty()
    );
    assert!(
        overloads
            .match_call(&[CallArgument::named(&ColumnName::new("exact"), &integer)])
            .unwrap()
            .is_empty()
    );
    let array = ArrayType::new(&integer);
    let parameter = test_helpers::parameter("xs", &array, ParameterMode::variadic(), false);
    let overloads = FunctionOverloads::new(&[test_helpers::signature(&[parameter], &integer)]);
    assert!(
        overloads
            .match_call(&[
                CallArgument::positional(&array).with_variadic(),
                CallArgument::positional(&integer),
            ])
            .unwrap()
            .is_empty()
    );
}
