# ARRAY query migration inventory

These tests arrived in [#761](https://github.com/kazuk/sql-fun/pull/761) before
this migration merged. The source is
[`array_subquery.rs`](../../sql-fun-sqlast/src/ast/resolve/query/tests/array_subquery.rs).
The fixture [array-subqueries.json](array-subqueries.json) stores 25 named case
objects and runs against PostgreSQL 17 and 18. The two isolated expressions from
[`builtin_bootstrap.rs`](../../sql-fun-sqlast/tests/builtin_bootstrap.rs) are in
[bootstrap-array-expressions.json](bootstrap-array-expressions.json), selected
only for PostgreSQL 18 to match the original version-specific checks.

## Migrated observable assertions

| Original test/cases | CLI contract and retained Rust coverage |
| --- | --- |
| `array_query_constructs_non_null_array`: `one`, `empty`, `many`, `null`, `unknown_null`, `unknown_string`, `array`, `empty_array_result`, `nested`, `star`, `clauses` | Result type, name and non-null array metadata move to the CLI. Rust is renamed `array_query_retains_child_type_domain_identity` and retains child width and the identity between the internal result domain and column type, with inference disabled/enabled. |
| `invalid_output_shape_retains_query_and_reports_problem`: `zero`, `two`, `star` | Exact `array_subquery_columns` diagnostic, absent result type and unknown nullability move to the CLI. Rust retains the invalid child width and empty result domain, with both inference settings. |
| `child_parameters_are_visited_and_finalized`: `projection`, `predicate`, `order`, `correlated`, `nested`, `repeated` | Public result columns move to the CLI, including the second column in `repeated`. Rust retains parameter admissibility and the finalized expression visitor checks. |
| `unconstrained_source_keeps_functional_array_relationship` | CLI checks unresolved public type and non-null array output. Rust retains the unrestricted parameter and functional result-domain relationship. |
| `anonymous_record_identity_is_retained_in_array_type` | CLI checks the absent public type and non-null array output. Rust retains the owned anonymous element definition; the API does not serialize this dynamic type as a named `record[]`. |
| `correlated_binding_and_serialized_query_survive_analysis` | CLI checks typed result columns without diagnostics. Rust retains the correlated child binding, child type and serialization round trip. |
| `enclosing_expressions_consume_array_result`: `subscript`, `cast` | Type, name and nullability move to the CLI; these result-only Rust cases are removed. |
| `builtin_bootstrap`: isolated ARRAY and CASE expressions | Exact SQL and `name[]` output move to the PostgreSQL 18 CLI fixture. The bootstrap test retains whole-catalog diagnostics and its `pg_policies.roles` check. |

The typed-parameter cases and unconstrained source also expect the CLI's
`parameter_type_unresolved` diagnostic. The product boundary must select a wire
parameter type; an explicitly cast expression can have a known output while its
input domain still admits multiple PostgreSQL types. Rust continues to test the
semantic admissibility domains rather than replacing them with that diagnostic.

PostgreSQL 17/18 oracle checks confirmed all eleven construction result types and
non-null array containers and all three invalid-width rejections. An array's
non-null container does not imply non-null elements.

## Retained cases and non-equivalent boundaries

| Original test/cases | Reason retained in Rust |
| --- | --- |
| `array_query_projection_preserves_both_source_shapes_and_correlations` | Retains scalar/array parameter alternatives and the narrowed `int4[]` result in the controlled context. The builtin CLI reports `parameter_type_unresolved` and an absent result type, so no equivalent fixture replaces this assertion. |
| `enclosing_expressions_consume_array_result::parameter_subscript` | Retained as `parameter_subscript_consumes_array_subquery_result`, asserting scalar `int4`. CLI nullability is unsound for NULL elements ([#762](https://github.com/kazuk/sql-fun/issues/762)); no exact fixture pins its incorrect `not_null` metadata. PostgreSQL 17/18 return NULL for a NULL parameter and for indexing an empty ARRAY query result. |
| `enclosing_expressions_consume_array_result::function` | Retained as `synthetic_identity_function_consumes_array_subquery_result`; `identity()` exists only in the controlled helper catalog. Replacing it with a builtin would change the scenario. |
| `invalid_array_query_does_not_stop_following_statement` | Retains sequential analysis and shared-context recovery. Independent CLI fixture inputs do not replace that state-transition contract. The invalid/valid SQL forms already appear in the CLI width/construction cases. |
| `named_composite_elements_keep_nominal_identity`: `named`, `named_pair` | Uses explicit custom composite DDL; outside the accepted builtin-only scope. |
| `deferred_star_checks_final_output_width`: `one`, `two` | Uses custom composites/functions and asserts the expanded child width and failed result domain. |
| `deferred_star_preserves_enclosing_result_link` | Uses custom composites/functions to verify the internal result link survives expansion. |
| `nested_deferred_arrays_finish_after_each_projection_expands` | Uses nested custom composite projections and verifies finalization after successive expansions. |

The retained mismatches do not block the other migrations. No production
inference or nullability fix, fallback, or diagnostic suppression is included.
