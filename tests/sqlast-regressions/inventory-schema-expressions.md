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
| `arrays::schema_types_and_bindings_survive_frozen_contexts`: `domain`, `domain_base`, `domain_unknown`, `array_domain`, `array_of_domains`, `enum_values`, `composite`, `nullable_column` | Exact public array type identity and constructor nullability. `array_of_domains` preserves `public.amount[]`; see [the common-type regression coverage](#postgresql-common-type-regressions). |
| `case_parameters::schema_result_types_survive_inference`: `domain`, `domain_base`, `domain_unknown`, `enum_values` | CASE result type identity and nullability. |
| `coalesce_parameters::catalog_types_survive_inference`: `same_domain`, `domain_base`, `domain_unknown`, `enum_values` | COALESCE result type identity. The CLI captures `parameter_type_unresolved` for these untyped parameters; Rust retains the source-context success and output-type assertion because those diagnostics are additional. |
| `min_max::schema_types_preserve_common_type_rules`: `domain`, `array_domain`, `array_domain_nested`, `domain_base`, `domain_array`, `domain_unknown`, `enum_values`, `composite` | GREATEST/LEAST result type identity and nullability. `domain_array` preserves `public.amount[]`; see [the common-type regression coverage](#postgresql-common-type-regressions). |

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

The original migration covered 36 SQL inputs across these fixture groups. For
COALESCE parameter cases, the CLI reports `parameter_type_unresolved` while
retaining the known result type. Rust keeps its source-context success and
output-type assertions alongside these CLI cases; the source and CLI diagnostics
are not equivalent. The Rust source context separately checks admissible
parameter domains; PostgreSQL's single untyped PREPARE parameter signature is a
different contract from those admissible domains, as documented in the
[type-system guideline](../../docs/internal-rules/sqlast-type-system-guideline.md#deferred-parameter-constraints).

## PostgreSQL common-type regressions

[Issue #767](https://github.com/kazuk/sql-fun/issues/767) corrects the two
migrated ARRAY/GREATEST expectations from `pg_catalog.int4[]` to
`public.amount[]`. The shared schema-expressions fixture now has 72 cases,
including 48 additional common-type regressions for ARRAY, GREATEST, LEAST,
COALESCE and parameter-dependent CASE. The original fixture case names remain
unchanged.

The matrix checks both operand orders, identical domain arrays, unknown NULL
operands in either position, scalar domain/base mixtures, domains over arrays,
nested domains over arrays, nested domain elements and numeric promotion.
CASE uses a boolean condition parameter to exercise its existing shared
common-type path, which considers ELSE before THEN. Thus a domain-element array
can remain the common type when it appears first; reversing the order can select
the base array instead. Domain-over-array controls still select ordinary `integer[]`.

The 50 matrix expressions (including the two corrected cases) were checked
with `pg_typeof` on PostgreSQL 15 through 18 using small transactions and
rollback-only schema setup; parameter-dependent CASE was also checked through
PREPARE/EXECUTE with an explicit boolean condition type. The maintained CLI
fixture runs offline against the supported PostgreSQL 17 and 18 catalogs in
source and installed checks.
Rust common-type tests retain coercion/candidate identity, owned-fact lifetime,
and correlated parameter/input/result-domain coverage with a minimal catalog.

Parameter-free CASE has a separate legacy candidate-enumeration path, which
rejects domain/base mixtures and can lose the result type for unknown NULL
branches. [Issue #776](https://github.com/kazuk/sql-fun/issues/776) records
reproductions, PostgreSQL results and the path distinction. That independent
resolver repair is outside #767; the CASE matrix here checks the shared path
without changing or suppressing the legacy behavior.
