# Schema-dependent domain and overload inventory

This inventory maps the custom-schema and domain cases moved from
[`domain_casts.rs`](../../sql-fun-sqlast/tests/domain_casts.rs) and
[`type_metadata_regressions.rs`](../../sql-fun-sqlast/tests/type_metadata_regressions.rs)
to the SQL CLI regression path. The cases use the real extracted catalog plus
their checked-in SQL schema fixture. The query JSON keeps each original case
identity beside its SQL and exact diagnostics/result columns.

## Domain casts

| Original cases | Fixture | Migrated public contract | Rust assertions retained |
| --- | --- | --- | --- |
| `explicit_cast_retains_domain_binding`: `identity`, `smallint`, `bigint`, `string_io`, `identifier`, `timestamp`, `null` | [`schema-domains-builtin.json`](schema-domains-builtin.json), default schema level | Seven casts to shipped `information_schema` domains, including I/O conversion and NULL, retain their exact result type and nullability. No schema DDL redeclares these shipped types. | The AST cast target, argument, and resolution serialization remain checked. |
| `explicit_cast_retains_domain_binding`: `source_domain`, `same_domain`, `nested`, `empty_array`, `array_domain`, `domain_elements` | [`schema-domains-project.json`](schema-domains-project.json) with [`schema-domains.sql`](schema-domains.sql), project schema level | Six project-domain casts assert the output's domain/array identity and nullability. | The serialized cast metadata remains checked. |
| `invalid_cast_keeps_target_lookup_and_argument`: `invalid`, `invalid_array`, `missing` | [`schema-domains-builtin.json`](schema-domains-builtin.json), default schema level | The two invalid explicit conversions and missing target assert the complete CLI diagnostic sets and retained result-column metadata. The missing target reports both `parameter_constraint` and `explicit_cast_not_defined`, and retains its unresolved target type. | Rust continues checking target resolution, cast arguments, and the internal diagnostic code. |
| `parameter_cast_preserves_admissible_inputs_and_domain_output` | [`schema-domains-project.json`](schema-domains-project.json) | The CLI checks the nested-domain result type. It reports `parameter_type_unresolved` and unknown nullability for the untyped parameter. | Admissible integer/domain inputs and the domain output constraint remain checked in Rust. |
| `expression_results_retain_qualified_domain_identity`: `cast`, `case_result`, `coalesce` | [`schema-domains-project.json`](schema-domains-project.json) | Cast, CASE, and COALESCE results retain the public `amount` identity. | No separate assertion was present beyond successful analysis and the result type. |
| `parameter_cast_accepts_the_qualified_domain_reference` | [`schema-domains-project.json`](schema-domains-project.json) | The CLI checks the public `amount` output type and records the unresolved input diagnostic. | The parameter assignment domain and admissible result domain remain checked in Rust. |

The project schema defines only `public.amount`, `public.nested_amount`, and
`public.amounts`. PostgreSQL's extracted catalog already supplies the four
`information_schema` domains, so the fixture uses those real definitions and
does not attempt to create conflicting replacements.

## Custom overload resolution

Each fixture has an isolated SQL schema file so it preserves the original
`choose` name, signature set, and declaration order without merging conflicting
function definitions.

| Original test group | Fixture(s) | Migrated public contract | Rust assertions retained |
| --- | --- | --- | --- |
| `function_choice_is_independent_of_catalog_order`: `left`, `right` | [`schema-domains-order-forward.json`](schema-domains-order-forward.json) and [`schema-domains-order-reverse.json`](schema-domains-order-reverse.json), each with its matching `.sql` file | Both input orders select `numeric`, independent of declaration order. | The original test contained no AST binding assertion. |
| `equal_function_ranks_remain_ambiguous`: `concrete`, `parameter` | [`schema-domains-equal-ranks.json`](schema-domains-equal-ranks.json) | Both equal-ranked calls retain the overload-resolution diagnostic. The parameter case also records its CLI `parameter_constraint` and `parameter_type_unresolved` diagnostics. | Rust retains the parameter case's original internal diagnostic check because the CLI reports additional parameter diagnostics. |
| `expanded_variadic_arguments_count_individually`: `exact`, `preferred`, `parameter` | [`schema-domains-variadic.json`](schema-domains-variadic.json) | Result types are `numeric`, `float8`, and `numeric`; the parameter case also reports `parameter_type_unresolved`. | Rust retains source-context success and result types, plus per-argument type checks when the source AST exposes a function call. The parameter case uses a different source expression representation, so this mixed group is not superseded. |
| `omitted_defaults_do_not_contribute_matches`: `preferred`, `exact`, `parameter` | [`schema-domains-defaults.json`](schema-domains-defaults.json) | Result types are `float8`, `numeric`, and `numeric`; the parameter case also reports `parameter_type_unresolved`. | Rust retains the parameter case's successful source-context analysis and `numeric` result type because the CLI reports an additional parameter diagnostic. |
| `ordinary_signature_precedes_identical_variadic_expansion`: `concrete`, `parameter` | [`schema-domains-variadic-precedence.json`](schema-domains-variadic-precedence.json) and [`schema-domains-variadic-precedence-reverse.json`](schema-domains-variadic-precedence-reverse.json) | Both declaration orders select the ordinary signature and return `int4`; parameter CLI cases report `parameter_type_unresolved`. | Rust retains the parameter case's successful source-context analysis and `int4` result type for both declaration orders. |
| `omitted_defaults_do_not_resolve_an_equal_input_tie` | [`schema-domains-default-tie.json`](schema-domains-default-tie.json) | The call retains `function_overload_resolution_failed`. | No separate binding assertion was present. |
| `extra_arguments_do_not_create_a_false_exact_match` | [`schema-domains-extra-arguments.json`](schema-domains-extra-arguments.json) | The two-argument call selects the `float8` result. | No separate binding assertion was present. |
| `output_parameters_do_not_contribute_input_matches`: `concrete`, `parameter` | [`schema-domains-out-parameters.json`](schema-domains-out-parameters.json) | The output parameter does not change input ranking; the result is `int4`. The parameter case also reports `parameter_type_unresolved`. | Rust retains the parameter case's successful source-context analysis and `int4` result type because the CLI reports an additional parameter diagnostic. |

`schema-domains-builtin.json` has 10 cases, `schema-domains-project.json` has 11,
and the isolated overload fixtures have 20 cases in total. All 41 cases passed
the SQL CLI contract on PostgreSQL 17 and 18 catalogs. The CLI's unresolved
parameter diagnostics are captured as emitted; where they differ from the
source-context diagnostic contract, the parameter-specific Rust assertion stays.

`same_type_cast_retains_view_column_lineage` stays in Rust. It asserts source
column lineage only, which the CLI result-column contract does not expose.
`loaded_preferred_cast_retains_existing_priority` also remains internal because
it inspects the cast metadata and overload-priority score directly. Builtin
function and parameter overload cases remain covered by the existing
[`metadata-overloads.json`](metadata-overloads.json) fixture and their internal
binding/constraint assertions.
