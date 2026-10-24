# ARRAY and min/max regression inventory

This inventory maps each original parameterized case in `arrays.rs` and
`min_max.rs` to the maintained SQL CLI regression contract or to retained Rust
coverage. CLI fixtures assert exact API type identities (`schema_name`, base
`local_name`, and `is_array`), projection names, nullability, and diagnostic-code
sets. An omitted SQL type is represented explicitly as `type: null`.

For parameter cases with a non-singleton inferred type domain, the CLI also
reports `parameter_type_unresolved`. The former Rust checks only asserted the
public result type and that semantic analysis had no problems; the CLI
diagnostic is an additional query-metadata/wire-output typing diagnostic, not
acceptance of a semantic regression. The exact CLI diagnostic sets continue to
reject any additional semantic errors. This applies to the `only`, `null`, and
`explicit` cases in `parameters_retain_uncertainty_or_use_explicit_target`, the
`parameter_cast` case in `projection_names`, all five cases in
`arrays-parameters.json`, and `an_explicit_parameter_cast_sets_the_result_type`
in `min_max.rs`.

Custom-schema cases remain in Rust under the accepted builtin-only scope.
[#761](https://github.com/kazuk/sql-fun/pull/761) resolved the earlier #727 catalog
initialization failure; no custom-schema fixture or substitute schema setup is
included in this batch.

## Scalar access nullability regressions (#762)

[arrays-consumers.json](arrays-consumers.json) adds twelve cases covering NULL
and empty constructors, out-of-range and NULL indices, NULL arrays,
multidimensional reads, deferred parameters and late element constraints. Scalar
reads conservatively expose nullable metadata, including the existing nonempty
subscript case. Single and mixed multidimensional slice controls preserve
array-valued results and non-null constructor-container metadata. Both PostgreSQL
17 and 18 catalogs run these cases through the shared manifest. The outer
`(ARRAY[$1])[1] + 1` expression retains an absent public type and unknown
nullability under the real catalogs; its exact unresolved-parameter diagnostic
is asserted without selecting a wire type. The retained
`late_constraints_reach_array_results` Rust test checks the admissible `int4`
result domain without asserting that it is the unique public type. The
anonymous-record array
subscript followed by an invalid field access also reports nullable metadata
through the existing member-access propagation, while preserving its
`composite_type_field_not_found` diagnostic and absent type.

## `arrays.rs`

| Original test and cases | Disposition |
| --- | --- |
| `metadata_matches_postgres`: `integers`, `mixed`, `mixed_reverse`, `nulls`, `strings`, `unknown`, `nullable`, `empty`, `cast_empty`, `parenthesized_empty`, `nested_empty`, `nested`, `nested_cast`, `array_operands`, `json`, `different_records`, `record_null`, `record_cast`, `preferred`, `varchar_first`, `bpchar_first` | Moved to [arrays-metadata.json](arrays-metadata.json); the Rust result-type/nullability-only test was removed after PostgreSQL 17 and 18 CLI runs passed. |
| `invalid_arrays_keep_failure_and_allow_next_statement`: `empty`, `categories`, `typed_string`, `nested_boundary`, `explicit_bad` | Diagnostic sets and result columns moved to [arrays-diagnostics.json](arrays-diagnostics.json). Rust retains diagnostic rendering, array-node value counts/type payload, and analysis of the following statement. |
| `parameters_retain_correlated_numeric_sources`: `mixed`, `reverse`, `repeated`, `subquery`, `cte`, `nested` | Retained for correlated admissible parameter sources and the semantic array node; those constraints and AST shape are not exposed by the CLI contract. |
| `unanchored_parameters_accept_typed_assignments`: `array`, `greatest`, `least`, `case`, each with `reverse = false` and `reverse = true` | Retained for parameter and result-column constraint domains, which the CLI does not expose. |
| `parameters_retain_uncertainty_or_use_explicit_target`: `only`, `null`, `explicit` | Moved to [arrays-parameters.json](arrays-parameters.json); the redundant Rust result-type test was removed. Each CLI case expects `parameter_type_unresolved` as described above. |
| `projection_names`: `implicit`, `alias`, `empty_cast`, `nested_cast`, `parameter_cast` | Moved to [arrays-names.json](arrays-names.json); the redundant Rust projection-name test was removed. The parameter case also expects `parameter_type_unresolved` as described above. |
| `schema_types_and_bindings_survive_frozen_contexts`: `domain`, `domain_base`, `domain_unknown`, `array_domain`, `array_of_domains`, `enum_values`, `composite`, `nullable_column` | Retained unchanged under the builtin-only scope because each case needs custom DDL and project schemas are outside this migration. |
| `anonymous_record_arrays_keep_their_element_fields`: `single`, `repeated`, `nested` | [arrays-records.json](arrays-records.json) checks the public name, nullability, and absent type. Rust retains the internal dynamic array type, record field definition, and element-count assertions. |
| `constructed_types_reach_existing_consumers`: `subscript`, `array_function`, `record_array_function` | Moved to [arrays-consumers.json](arrays-consumers.json); the Rust result-type-only test was removed. |
| `anonymous_records_do_not_implicitly_become_named_composites`: `scalar`, `array` | Retained unchanged under the builtin-only scope; the overload candidates require custom type/function DDL. |
| `competing_record_overloads_select_postgres_candidate`: `anonymous`, `named`, `anonymous_array`, `named_array`, `constructed_named_array`, `anonymous_polymorphic` | Retained unchanged under the builtin-only scope; result selection depends on custom composite types and function overload declarations. |
| `record_overloads_preserve_postgres_ambiguity`: `scalar`, `array` | Retained unchanged under the builtin-only scope; ambiguity depends on custom function declarations. |
| `unanchored_arrays_keep_owned_constraints`: `single`, `null`, `string`, `repeated`, `nested`, `subquery` | Retained for owned parameter-constraint domains after the analysis context is dropped. |
| `unanchored_array_inputs_remain_jointly_constrained`: `direct`, `nested`, `cte` | Retained for joint multi-parameter compatibility, which the CLI contract does not expose. |
| `late_constraints_reach_array_results`: `predicate`, `cte`, `subscript` | Retained for late parameter/result-column constraint propagation. |
| `ordinary_array_equality_retains_binding_and_types`: `constructor`, `integers`, `empty`, `null_array` | Public result columns moved to [arrays-operators.json](arrays-operators.json). Rust retains resolved operator binding and argument/result/strictness metadata. |
| `ordinary_array_equality_retains_binding_and_types`: `catalog_shape` | Retained under the builtin-only scope because its source column comes from a custom table definition; the same Rust test continues checking binding metadata. |
| `invalid_array_equality_retains_failure`: `different_elements`, `numeric_elements`, `scalar_right`, `scalar_left` | Diagnostic sets and absent result types moved to [arrays-diagnostics.json](arrays-diagnostics.json). Rust retains the unresolved-operator AST variant, resolution state, and operand payloads. |

The non-parameterized `conflicting_parameter_use_reports_a_constraint_failure`
test moved to `arrays-diagnostics.json` and was removed. Its parameter-constraint
diagnostic is now checked through the CLI.

The non-parameterized `parameter_with_anonymous_records_uses_the_record_family`
case has its public `record[]` output checked in `arrays-records.json`; Rust
retains admissible parameter-family coverage. The non-parameterized
`public_array_parameter_domains_do_not_select_an_untyped_prepare_signature`
case has absent/narrowed public result types checked in `arrays-parameters.json`;
Rust retains the parameter and joint-constraint assertions. Both CLI cases
expect `parameter_type_unresolved` as described above.

`operands_keep_successful_and_failed_column_bindings` and
`declared_composite_array_fields_use_shipped_catalog_types` remain unchanged
under the builtin-only scope because their source tables/composite types require
custom DDL.
The non-parameterized `subscripted_anonymous_record_has_no_sql_field_descriptor`
test moved to `arrays-diagnostics.json` and was removed.

## `min_max.rs`

| Original test and cases | Disposition |
| --- | --- |
| `result_metadata_matches_postgres`: `single`, `numeric`, `numeric_reverse`, `null_first`, `null_last`, `all_null`, `order_left`, `order_right`, `strings`, `unknown_string`, `typed_null`, `preferred`, `range`, `multirange`, `vector`, `arrays`, `nested` | Result types/nullability moved to [minmax-metadata.json](minmax-metadata.json). The remaining Rust structural test was renamed `operation_and_arguments_are_retained`; it checks operation name, concrete common-type state, and operand presence. |
| `invalid_operands_report_semantic_problems`: `types`, `typed_string`, `preferred_category`, `json`, `parameter_json`, `parameter_json_reverse`, `parameter_json_array`, `dynamic_left`, `dynamic_right`, `json_array` | Diagnostic sets and result columns moved to [minmax-diagnostics.json](minmax-diagnostics.json). The remaining Rust rendering test was renamed `invalid_operands_render_specific_diagnostics`; it retains code lookup needed to check rendered diagnostic text. |
| `schema_types_preserve_common_type_rules`: `domain`, `array_domain`, `array_domain_nested`, `domain_base`, `domain_array`, `domain_unknown`, `enum_values`, `composite` | Retained unchanged under the builtin-only scope because each case depends on custom DDL. |
| `parameter_sources_allow_numeric_promotion`: `numeric`, `nested`, `repeated`, `subquery`, `cte` | Retained for the admissible parameter source types and their correlation. |
| `comparison_catalog_distinguishes_absence_from_negative_answers`: `ordered`, `not_ordered`, `unknown_identity` | Retained because these are catalog-fact queries, not SQL result-column contracts. |
| `dynamic_composite_operands_retain_their_type`: `greatest`, `least` | [minmax-metadata.json](minmax-metadata.json) checks API name, nullability, and absent type. Rust retains the internal dynamic record identity and min/max operand structure that the API type field cannot represent. |

The non-parameterized `incompatible_parameter_uses_report_constraints` case
moved to `minmax-diagnostics.json` and was removed. The non-parameterized
`an_explicit_parameter_cast_sets_the_result_type` case now keeps its internal
argument-count assertion while `minmax-metadata.json` checks the public result
type and expects `parameter_type_unresolved` as described above.
`later_operands_are_resolved_even_after_non_null_literal` now keeps only
the unresolved AST binding assertion; its diagnostic and result-column contract
is in `minmax-diagnostics.json`.

`unanchored_parameters_retain_alternatives_and_keep_minmax` remains for
parameter compatibility and expression shape. `later_operands_retain_column_bindings`
and `nullable_operands_do_not_prove_non_null` remain unchanged under the
builtin-only scope because they require custom table DDL.
