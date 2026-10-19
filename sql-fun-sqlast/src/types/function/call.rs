//! Call-shaped filtering without casts, polymorphic binding or ranking.

use super::{FunctionOverloads, FunctionReadError, FunctionSignature, ParameterMode};
use crate::sem::ColumnName;
use crate::types::array::ArrayType;
use crate::types::{Candidates, TypeDefinition, TypeReference};

/// One syntactically supplied argument, in call order, with its type candidates.
#[derive(Debug, Clone)]
pub struct CallArgument {
    name: Option<ColumnName>,
    candidates: TypeReference,
    variadic: bool,
}

impl CallArgument {
    /// A positional argument. Candidate membership uses `same_type`.
    pub fn positional(candidates: &dyn TypeDefinition) -> Self {
        Self {
            name: None,
            candidates: candidates.clone_box(),
            variadic: false,
        }
    }
    /// A named argument, using an already normalized logical SQL name.
    pub fn named(name: &ColumnName, candidates: &dyn TypeDefinition) -> Self {
        Self {
            name: Some(name.clone()),
            candidates: candidates.clone_box(),
            variadic: false,
        }
    }
    /// Mark this final argument as an explicit SQL `VARIADIC` array argument.
    pub const fn with_variadic(mut self) -> Self {
        self.variadic = true;
        self
    }
}

#[derive(Debug, Clone)]
struct MatchedSignature {
    signature: FunctionSignature,
    arguments: Vec<TypeReference>,
}

/// Surviving whole signatures and the expected types of syntactically supplied values.
///
/// Mapping is retained per signature: named reordering and expanded variadic inputs
/// never turn into declaration indices. Provider order, duplicates and scopes survive.
#[derive(Debug, Clone)]
pub struct FunctionCallMatches(Vec<MatchedSignature>);

impl FunctionCallMatches {
    /// Whole surviving signatures, suitable for further declaration-level reads.
    pub fn overloads(&self) -> FunctionOverloads {
        let signatures: Vec<_> = self.0.iter().map(|m| m.signature.clone()).collect();
        FunctionOverloads::new(&signatures)
    }
    /// Whether no candidate can accept the supplied call.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Expected types at a zero-based syntactic call position, including named values.
    /// Expanded variadic values project element types; explicit VARIADIC projects arrays.
    /// Missing positions and empty matches produce `Never`.
    pub fn argument_candidates(&self, index: usize) -> TypeReference {
        let values: Vec<_> = self
            .0
            .iter()
            .filter_map(|m| m.arguments.get(index).cloned())
            .collect();
        Candidates::from_candidates(&values)
    }
    /// Further constrain one syntactic value while retaining correlated signatures.
    pub fn filter_argument(&self, index: usize, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|m| {
                    m.arguments
                        .get(index)
                        .is_some_and(|t| FunctionOverloads::member(candidates, t.as_ref()))
                })
                .cloned()
                .collect(),
        )
    }
    /// Result candidates of surviving whole signatures; procedures contribute nothing.
    pub fn result_candidates(&self) -> TypeReference {
        self.overloads().result_candidates()
    }
    /// Further constrain results while retaining the original call mapping.
    pub fn filter_result(&self, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|m| {
                    m.signature
                        .result()
                        .result_type()
                        .is_some_and(|r| FunctionOverloads::member(candidates, r))
                })
                .cloned()
                .collect(),
        )
    }
}

impl FunctionOverloads {
    /// Filter complete calls, accounting for names, defaults and variadic expansion.
    ///
    /// Arguments are in syntax order: positional values precede named values, and
    /// explicit VARIADIC is last. Duplicate assignment, unknown names, missing
    /// required inputs, excess inputs and invalid syntax shapes retain no signature.
    /// OUT/TABLE parameters never consume values. An omitted variadic parameter
    /// requires a declared default (use explicit VARIADIC for an empty array).
    /// Ambiguous declaration names return an error rather than choosing a parameter.
    ///
    /// Type matching uses direct `same_type` membership, as in `filter_argument`.
    /// Supply cast-expanded candidates explicitly when needed. This operation does
    /// not solve polymorphic constraints, rank survivors or select an overload.
    pub fn match_call(
        &self,
        arguments: &[CallArgument],
    ) -> Result<FunctionCallMatches, FunctionReadError> {
        let mut matches = Vec::new();
        for signature in &self.0 {
            if let Some(types) = signature.match_arguments(arguments)? {
                matches.push(MatchedSignature {
                    signature: signature.clone(),
                    arguments: types,
                });
            }
        }
        Ok(FunctionCallMatches(matches))
    }
}

#[cfg(test)]
mod tests {
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
        let overloads =
            FunctionOverloads::new(&[test_helpers::signature(&[a.clone(), b], &integer)]);
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
        let ambiguous =
            FunctionOverloads::new(&[test_helpers::signature(&[a.clone(), a], &integer)]);
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

    /// PostgreSQL 15–18 reject omission without a default, even for variadic-only
    /// functions; an explicit empty VARIADIC array or a declared default permits it.
    #[rstest::rstest]
    fn zero_variadic_values_follow_postgresql(
        #[values(false, true)] preceding_fixed: bool,
        #[values(false, true)] has_default: bool,
        #[values(false, true)] explicit_empty_array: bool,
    ) {
        let integer = test_helpers::integer();
        let array = ArrayType::new(&integer);
        let default = if has_default {
            Some(DefaultExpression::sql_list("ARRAY[]::integer[]", 0))
        } else {
            None
        };
        let mut parameters = Vec::new();
        let mut arguments = Vec::new();
        if preceding_fixed {
            parameters.push(test_helpers::parameter(
                "a",
                &integer,
                ParameterMode::input(),
                false,
            ));
            arguments.push(CallArgument::positional(&integer));
        }
        parameters.push(FunctionParameter::new(
            &Some(ColumnName::new("xs")),
            ParameterMode::variadic(),
            &array,
            &default,
        ));
        if explicit_empty_array {
            arguments.push(CallArgument::positional(&array).with_variadic());
        }
        let signature = test_helpers::signature(&parameters, &integer);
        let overloads = FunctionOverloads::new(std::slice::from_ref(&signature));
        let matched = overloads.match_call(&arguments).unwrap();
        assert_eq!(!matched.is_empty(), has_default || explicit_empty_array);
        if has_default || explicit_empty_array {
            assert_eq!(matched.overloads().signatures(), &[signature]);
            // Omission creates no synthetic call argument. Explicit arrays keep a slot.
            assert!(
                matched
                    .argument_candidates(arguments.len())
                    .as_any()
                    .is::<Never>()
            );
            if explicit_empty_array {
                assert!(
                    matched
                        .argument_candidates(arguments.len() - 1)
                        .as_any()
                        .downcast_ref::<Candidates>()
                        .unwrap()
                        .contains(&array)
                );
            }
        } else {
            assert!(matched.argument_candidates(0).as_any().is::<Never>());
            assert!(matched.result_candidates().as_any().is::<Never>());
        }
    }
}

impl FunctionSignature {
    fn match_arguments(
        &self,
        arguments: &[CallArgument],
    ) -> Result<Option<Vec<TypeReference>>, FunctionReadError> {
        let inputs: Vec<_> = self
            .parameters
            .iter()
            .filter(|p| p.mode().accepts_input())
            .collect();
        let mut used = vec![false; inputs.len()];
        let mut expected = Vec::new();
        let mut position = 0;
        let mut named = false;
        for (index, argument) in arguments.iter().enumerate() {
            if argument.variadic && index + 1 != arguments.len() {
                return Ok(None);
            }
            let target = if let Some(name) = &argument.name {
                named = true;
                let Some(parameter) = self.argument_by_name(name)? else {
                    return Ok(None);
                };
                let Some(target) = inputs.iter().position(|p| std::ptr::eq(*p, parameter)) else {
                    return Ok(None);
                };
                target
            } else {
                if named {
                    return Ok(None);
                }
                position
            };
            let Some(parameter) = inputs.get(target) else {
                return Ok(None);
            };
            let variadic = parameter.mode() == ParameterMode::variadic();
            if argument.variadic && !variadic {
                return Ok(None);
            }
            let expanded = variadic && !argument.variadic;
            // Named variadic parameters require explicit array passing in PostgreSQL.
            if expanded && argument.name.is_some() {
                return Ok(None);
            }
            if used[target] && (!expanded || argument.name.is_some()) {
                return Ok(None);
            }
            let type_ = if expanded {
                let Some(array) = parameter
                    .parameter_type()
                    .as_any()
                    .downcast_ref::<ArrayType>()
                else {
                    return Ok(None);
                };
                array.element_type()
            } else {
                parameter.parameter_type()
            };
            if !FunctionOverloads::member(argument.candidates.as_ref(), type_) {
                return Ok(None);
            }
            used[target] = true;
            expected.push(type_.clone_box());
            if !named && !expanded {
                position += 1;
            }
        }
        if inputs
            .iter()
            .zip(used)
            .any(|(p, used)| !used && p.default_expression().is_none())
        {
            return Ok(None);
        }
        Ok(Some(expected))
    }
}
