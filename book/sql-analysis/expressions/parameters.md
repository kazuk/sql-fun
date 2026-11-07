# Parameter inference and explicit hints

## Overview

Numbered parameters (`$1` in PostgreSQL and `?1` in SQLite) can be used across
supported nonrecursive CTEs and the outer statement, including INSERT sources,
RETURNING, and conflict predicates. Repeated uses of a parameter must have
compatible SQL type requirements; conflicts produce diagnostics. A parameter
used only by `IS NULL` has no particular type requirement. Explicit SQL casts
constrain accepted input types. SQLite casts follow SQLite semantics and do not
require PostgreSQL cast rules.

PostgreSQL analysis accounts for implicit casts in operator and ordinary
function calls. A parameter or a parameter-dependent result column may have
several admissible types. These are alternatives for explicitly
typed inputs, rather than PostgreSQL's single signature inferred by an untyped
`PREPARE`. For example, `SELECT ARRAY[$1,1]` admits an `integer` input with an
`integer[]` result and a `bigint` input with a `bigint[]` result. PostgreSQL
chooses `integer` when preparing that statement without parameter types.
When several result types remain possible, analysis cannot report one definite
result type. An explicit SQL cast can narrow the result type. Once an application
input type is selected, reported result columns use that selection. Other
explicitly typed inputs may still be SQL-admissible with different result types.

For application bindings, an explicit `${name:typehint}` is authoritative.
Analysis validates that type against every use and never substitutes another
type to make the query work. A hint applies to the parameter everywhere, even
when written on a later occurrence.

Without a hint, PostgreSQL application binding selection uses the parameter's
**first syntactic occurrence**, including occurrences in nested queries and
CTEs. Only that occurrence's local expression selects the input type. At a
consuming operation, the candidates are `bool`, `int4`, `int8`, `float4`,
`float8`, `text`, and `numeric`. These seven types are sql-fun's bounded
selection policy, not the complete set of types PostgreSQL clients can supply.

The local operation determines which candidates it accepts: an explicit cast
allows explicit conversions, while function and operator arguments use their
applicable implicit conversion rules. A unique candidate requiring no
conversion wins. If no identity candidate exists, a single remaining
admissible candidate wins. Multiple remaining choices produce
`parameter_type_unresolved`, listing the candidates. An occurrence with no
input-type requirement also remains ambiguous. Later uses cannot resolve that
ambiguity or change an earlier selection.

```sql
SELECT ${src}::text AS value;
```

This selects a `text` input through identity acceptance, so an additional
`${src:text}` hint is unnecessary. The same applies when the cast supplies an
argument to a record-returning table function. Native `$1::text` uses the same
binding rule. Use one placeholder style per input; do not mix named
placeholders with native `$1` parameters.

```sql
SELECT ${src}::text, ${src} + 1;
```

The first occurrence selects `text`. The later addition is checked as
`text + integer` and reports no matching operator overload; analysis does not
search for a different input type satisfying both occurrences. Conflicting
uses report the selected binding and the conflicting type requirements.
A selected binding can therefore remain known while the statement is incomplete
because of another error.

```sql
SELECT ${src} IS NULL, ${src}::text;
```

The first occurrence does not choose an input type. The later cast cannot
repair that ambiguity; supply an explicit `${src:text}` hint when a text
binding is intended.

Explicit hints may supply types outside the seven defaults, including arrays,
composite types, user-defined types, and `bytea`, when accepted by the query.
SQL-admissible input alternatives and application binding selection are separate:
SQL that has no selected application binding is not necessarily invalid SQL.
Known result-column metadata remains available even with an unresolved input.
PostgreSQL's untyped `PREPARE` inference is not the definition of sql-fun's
application binding rules.

Unresolved expression results are distinct from parameter placeholders. If
resolving them would require a catalog-wide type search, analysis reports
`inference_candidate_space_unsupported`. This indicates a limitation of
inference, not invalid SQL. Supply SQL type context or an input type hint.
For `bytea`, the protocol's text value format does not change its SQL type to
`text`; use an explicit `bytea` input hint to select that binding.

Parameters in supported ORDER BY, GROUP BY, HAVING, DISTINCT ON, LIMIT/OFFSET,
aggregate FILTER/ordering and window expressions participate in the same query
analysis, including nested queries. See [common clauses](../clauses.md) for
binding precedence, grouping/window legality and incomplete RANGE analysis.
[Function calls](function-calls.md), [conditional expressions](conditional.md)
and [arrays](arrays-and-rows.md) own their operation-specific constraints.

Explicit input hints name SQL types in the selected dialect and search path.
PostgreSQL accepts aliases such as `INTEGER`, qualified names such as
`pg_catalog.int4`, quoted names and array types. Hints specify input types, not
column declarations: lengths, precision/scale and other type modifiers are
rejected. Hints must satisfy all uses of the parameter and do not insert casts
or change executable SQL. Hint syntax available to an application depends on its
[CLI](../../cli/analyze.md) or [query API](../../api/check-analysis-design.md)
input format.

## Dialect and engine-specific behavior

PostgreSQL implicit-cast and overload rules above do not become SQLite typing
rules. Native SQLite numbered parameters (`?1`) participate in supported syntax,
but operator-dependent inference and parameter-dependent CASE/COALESCE result
typing remain incomplete. SQLite casts use affinity semantics, not PostgreSQL
cast declarations. SQLite analysis in the library requires the `sqlite` feature;
it does not establish SQLite availability through every server/CLI workflow.

## Diagnostics

- [invalid_parameter_hint]: an explicit hint cannot be parsed or resolved as a
supported client type.
- [parameter_constraint]: the joint SQL requirements cannot be satisfied.
- [parameter_type_unresolved]: application binding finalization has no single
input type.
- [inference_candidate_space_unsupported]: resolving a non-placeholder
expression would require unsupported catalog-wide inference.

[invalid_parameter_hint]: ../../diagnostics/invalid_parameter_hint.md
[parameter_constraint]: ../../diagnostics/parameter_constraint.md
[parameter_type_unresolved]: ../../diagnostics/parameter_type_unresolved.md
[inference_candidate_space_unsupported]: ../../diagnostics/inference_candidate_space_unsupported.md

Conflicting type requirements and the absence of any type requirement are
different conditions. A conflict does not erase an already selected application
input type; an unconstrained first occurrence does not select one.
