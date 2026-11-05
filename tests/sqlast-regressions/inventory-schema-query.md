# Schema-dependent query regression inventory

This inventory maps schema-dependent query assertions from
[`qualified_columns.rs`](../../sql-fun-sqlast/tests/qualified_columns.rs) and
[`select_statement/ast.rs::test_from_ast::array_subquery`](../../sql-fun-sqlast/src/sem/select_statement/ast.rs)
and table-function assertions from
[`select_statement/ast.rs::test_from_ast::table_functions`](../../sql-fun-sqlast/src/sem/select_statement/ast.rs)
to the custom-schema CLI fixtures introduced for issue #764. The fixtures use
the real CLI analysis path with project schema DDL; they do not execute SQL.

## Qualified column resolution

[`schema-qualified.json`](schema-qualified.json) contains 25 named cases and
uses [`schema-qualified.sql`](schema-qualified.sql). The SQL-visible
diagnostic, result type, and nullability assertions are checked by the CLI on
PostgreSQL 17 and 18. Rust keeps semantic AST and binding assertions where they
test information the CLI contract cannot expose.

| Original Rust case | Fixture case | Rust disposition |
| --- | --- | --- |
| `binds_canonical_table_identity::schema` | Same case name in `schema-qualified.json` | Retains canonical relation binding. |
| `binds_canonical_table_identity::aliased_other_occurrence` | Same case name | Retains binding to the unaliased occurrence. |
| `binds_canonical_table_identity::quoted` | Same case name | Retains the canonical quoted schema/table identity. |
| `same_named_relations_keep_distinct_qualified_bindings` | Same SQL; two fixture result columns check `int4` and `int8` | Retains the distinct `FullName` binding and corresponding column type for each result. |
| `retains_failed_qualified_lookup::{alias,same_alias,absent_from,wrong_schema,cte,derived,join_alias,duplicate_source}` | Same case names | Retains each completed qualification failure, its code, the absent result type, and the referenced column. `duplicate_source` also reports `duplicate_alias` on the CLI path; the fixture records both observed diagnostics while Rust retains `column_source_ambiguous` in the AST. |
| `preserves_source_type_and_nullability::{view,left_join,right_join,full_join,inner_join}` | Same case names | Result-only assertions move to the CLI fixture. |
| `correlated_references_obey_relation_visibility::{correlation,local_alias_does_not_mask,hidden_local_relation,missing_column}` | Same case names | Result-only assertions move to the CLI fixture. Issue [#765](https://github.com/kazuk/sql-fun/issues/765) restores the inner reference name (`id`, or retained `absent` on the diagnostic-bearing case). Nullable metadata and diagnostics are preserved; the successful cases match PostgreSQL 17/18 output names. |
| `dml_consumes_the_same_qualified_bindings::{insert,update,delete}` | Same case names | Retains each DML `RETURNING` relation binding; diagnostics and output metadata move to the CLI fixture. |
| `missing_column_retains_the_selected_relation` | Same case name | Retains the selected relation binding; the diagnostic and absent result type move to the CLI fixture. |
| `binds_canonical_table_identity::search_path` | `-` | Retained because the case uses `search_path = ['a', 'public']`, while the public regression runner configures `public`. The SQL is not rewritten. |
| `binds_canonical_table_identity::database`, `retains_failed_qualified_lookup::cross_database`, `catalog_qualification_requires_explicit_database_identity` | `-` | Retained because these cases require explicit database identity, which the fixture runner does not configure. |
| SQLite cases in `qualified_columns.rs` | `-` | Retained at the SQLite analyzer and SQLite oracle boundary. They are not PostgreSQL schema fixtures. |

## Schema composites in ARRAY queries

[`schema-composites.json`](schema-composites.json) contains six named cases and
uses [`schema-composites.sql`](schema-composites.sql). The fixture checks the
SQL-visible nominal array types, width diagnostic, and enclosing result types
against PostgreSQL 17 and 18.

| Original Rust case | Fixture case | Rust disposition |
| --- | --- | --- |
| `named_composite_elements_keep_nominal_identity::{named,named_pair}` | Same case names | Result type assertions move to the CLI fixture. |
| `deferred_star_checks_final_output_width::{one,two}` | Same case names | Rust retains expanded child width and the failed empty result domain. The invalid `pair` case reports both `array_subquery_columns` and `parameter_constraint` through the CLI. |
| `deferred_star_preserves_enclosing_result_link` | Same case name | The test asserted only the public scalar result type, so it is removed after CLI migration. |
| `nested_deferred_arrays_finish_after_each_projection_expands` | Same case name | The test asserted only the public array result type and diagnostic set, so it is removed after CLI migration. |
| Other tests in `array_subquery.rs` | Existing fixtures or `-` | Existing builtin-only migrations are mapped in [`inventory-array-subqueries.md`](inventory-array-subqueries.md). Parameter domains, binding graphs, serialization, synthetic helper functions, and sequential-context state remain in Rust. |

The project schema fixture declares the composite types and the three typed
functions used to constrain deferred `$1` expansion. No analyzer behavior or
fallback was added to make these cases pass.

## Table-function output definitions

[`schema-table-functions.json`](schema-table-functions.json) contains 20 named
cases and uses [`schema-table-functions.sql`](schema-table-functions.sql). All
cases passed the CLI contract on PostgreSQL 17 and 18. PostgreSQL `PREPARE`
checks confirmed the declared output labels and types for the table-function
cases; the explicitly cast parameter is also inferred as `text` by PostgreSQL.

| Original Rust case | Fixture case | Rust disposition |
| --- | --- | --- |
| `resolves_declared_record_fields::{qualified,unqualified,star,qualified_star,subquery,cte,rows,rows_unqualified,rows_star,rows_outer_definition,rows_partial_alias,rows_implicit_alias,named_arguments,rows_named_arguments,rows_ordinality,collation}` | Same case names | Result-only type and diagnostic assertions move to the CLI fixture; the Rust cases are removed. |
| `resolves_declared_record_fields::parameter` | Same case name | The fixture records CLI result types and its `parameter_type_unresolved` diagnostic. Rust retains the controlled-context no-problem assertion. PostgreSQL 17 and 18 infer `$1` as `text` for the exact query under `PREPARE`; the distinction between CLI admissible input domains and PostgreSQL's chosen prepared-statement type is documented in [#766](https://github.com/kazuk/sql-fun/issues/766). |
| `preserves_declared_returns_and_positional_aliases::{scalar,composite,out}` | Same case names | Result-only type and diagnostic assertions move to the CLI fixture; the Rust cases are removed. |

The table-function fixture declares matching SQL signatures for the helper
functions and a named composite type. PostgreSQL 17 and 18 `\gdesc` output
matched the fixture's declared field names and scalar types.
