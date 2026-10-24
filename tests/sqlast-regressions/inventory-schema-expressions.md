# Schema-dependent expression and array inventory

This inventory maps custom-schema result and overload cases from
[`arrays.rs`](../../sql-fun-sqlast/tests/arrays.rs),
[`case_parameters.rs`](../../sql-fun-sqlast/tests/case_parameters.rs),
[`coalesce_parameters.rs`](../../sql-fun-sqlast/tests/coalesce_parameters.rs),
and [`min_max.rs`](../../sql-fun-sqlast/tests/min_max.rs) to the SQL CLI
regression path. Each fixture preserves original SQL and case identity and
asserts complete CLI diagnostic sets and ordered result-column metadata.

## Shared domain, enum, composite, and table setup

[`schema-expressions.json`](schema-expressions.json) uses
[`schema-expressions.sql`](schema-expressions.sql), which defines the types
needed by the original cases. The eight ARRAY cases assert the result array
type and non-null constructor metadata. Their Rust test remains for the
internal array-node value count.

| Rust test and cases | CLI result contract |
| --- | --- |
| `arrays::schema_types_and_bindings_survive_frozen_contexts`: `domain`, `domain_base`, `domain_unknown`, `array_domain`, `array_of_domains`, `enum_values`, `composite`, `nullable_column` | Exact public array type identity and constructor nullability. `array_of_domains` currently remains `pg_catalog.int4[]` in SQLAST, as in the original Rust assertion; see [the verified PostgreSQL discrepancy](#postgresql-common-type-discrepancy). |
| `case_parameters::schema_result_types_survive_inference`: `domain`, `domain_base`, `domain_unknown`, `enum_values` | CASE result type identity and nullability. |
| `coalesce_parameters::catalog_types_survive_inference`: `same_domain`, `domain_base`, `domain_unknown`, `enum_values` | COALESCE result type identity. The CLI captures `parameter_type_unresolved` for these untyped parameters; Rust retains the source-context success and output-type assertion because those diagnostics are additional. |
| `min_max::schema_types_preserve_common_type_rules`: `domain`, `array_domain`, `array_domain_nested`, `domain_base`, `domain_array`, `domain_unknown`, `enum_values`, `composite` | GREATEST/LEAST result type identity and nullability. `domain_array` currently remains `pg_catalog.int4[]` in SQLAST, as in the original Rust assertion; see [the verified PostgreSQL discrepancy](#postgresql-common-type-discrepancy). |

## Record overloads and composite array fields

Conflicting overload declarations remain in separate schema files. The fixture
SQL retains the original function names and signatures.

| Rust test and cases | Fixture | Retained internal assertions |
| --- | --- | --- |
| `arrays::anonymous_records_do_not_implicitly_become_named_composites`: `scalar`, `array` | [`schema-expressions-record-scalar.json`](schema-expressions-record-scalar.json) and [`schema-expressions-record-array.json`](schema-expressions-record-array.json), each with its matching `.sql` file | The original tests asserted only the overload failure, now covered by the exact CLI diagnostic. |
| `arrays::competing_record_overloads_select_postgres_candidate`: `anonymous`, `named`, `anonymous_array`, `named_array`, `constructed_named_array`, `anonymous_polymorphic` | [`schema-expressions-record-overloads.json`](schema-expressions-record-overloads.json) with its matching `.sql` file | The original cases asserted only result types. |
| `arrays::record_overloads_preserve_postgres_ambiguity`: `scalar`, `array` | [`schema-expressions-record-ambiguity.json`](schema-expressions-record-ambiguity.json) with its matching `.sql` file | The original cases asserted only the overload-failure diagnostic. |
| `arrays::declared_composite_array_fields_use_shipped_catalog_types` | [`schema-expressions-composite-array.json`](schema-expressions-composite-array.json) with its matching `.sql` file | The CLI checks both the field array type and indexed element type. |
| `arrays::ordinary_array_equality_retains_binding_and_types::catalog_shape` | [`schema-expressions-array-catalog-shape.json`](schema-expressions-array-catalog-shape.json) with its matching `.sql` file | Rust retains the resolved operator binding, operand types, boolean return type, and strictness. |

The eight ARRAY cases retain only their internal value-count assertion in Rust;
the CLI fixture owns their result type, nullability, and diagnostic contract.
The four CASE, two anonymous-record rejection, six competing
overload, two ambiguity, and composite-field result groups had no separate
internal assertion and were removed after their CLI cases passed. The catalog
shape equality Rust case stays because it checks operator binding metadata.

All 36 cases passed the SQL CLI contract on PostgreSQL 17 and 18 catalogs. For
COALESCE parameter cases, the CLI reports `parameter_type_unresolved` while
retaining the known result type. Rust keeps its source-context success and
output-type assertions alongside these CLI cases; the source and CLI diagnostics
are not equivalent. The Rust source context separately checks admissible
parameter domains; PostgreSQL's single untyped PREPARE parameter signature is a
different contract from those admissible domains, as documented in the
[type-system guideline](../../docs/internal-rules/sqlast-type-system-guideline.md#deferred-parameter-constraints).

## PostgreSQL common-type discrepancy

The CLI and original SQLAST assertions return `pg_catalog.int4[]` for both
`ARRAY[NULL::amount[],NULL::integer[]]` and
`greatest(NULL::amount[],NULL::integer[])`. PostgreSQL 17 and 18 return
`public.amount[]` for both expressions when `public.amount` is a domain over
`integer`. The difference is recorded in [issue #767](https://github.com/kazuk/sql-fun/issues/767).
This migration preserves and makes the current analyzer result observable; it
does not change common-type semantics.

The oracle checks used a transaction-local `CREATE DOMAIN public.amount AS
integer`, queried `pg_typeof` for both expressions on PostgreSQL 17 and 18, and
rolled back the DDL. Reproductions and observed results are recorded in #767.
