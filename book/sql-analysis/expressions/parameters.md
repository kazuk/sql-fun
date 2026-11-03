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
result type. An explicit SQL cast can narrow the result type.

Query analysis for an application must establish its input types. If SQL
inference or a supported explicit input type hint does not establish one input
type, it returns `parameter_type_unresolved` as an error and marks the statement
incomplete. This applies both when several input types remain possible and when
the SQL does not require any particular input type. If no type satisfies all
requirements, analysis reports the conflicting uses instead.

An unresolved binding does not mean the SQL is intrinsically invalid or that
PostgreSQL could not execute it with client-supplied parameter types.
PostgreSQL's untyped `PREPARE` signature is not automatically selected as the
application's binding. Known result-column metadata remains available even when
an input binding is unresolved. For example, `SELECT $1::text AS value` has a
known `text` result but its result cast need not determine a unique source type.

For named application parameters, `SELECT ${src:text}::text AS value` explicitly
supplies the input type and completes with a `text` binding. Likewise,
`record_fun(${src:text}::text)` resolves the input binding when `record_fun` has
a `text` argument. `${src}::text` only casts the result of the parameter
expression. Use one placeholder style per input; do not mix named placeholders
with native `$1` parameters. Explicit hints still undergo validation against all
uses of the parameter.

When overloaded operations have no SQL type information for their inputs,
PostgreSQL bind-input inference considers seven default candidates: `bool`,
`int4`, `int8`, `float4`, `float8`, `text`, and `numeric`. This list is not a
preference order: several surviving candidates still leave the binding
unresolved. Unrelated catalog types do not enlarge these default candidates. SQL
context and explicit input hints can establish other types, including arrays,
composite types, user-defined types, and `bytea`. All occurrences of a parameter
contribute constraints before its binding is finalized.
Explicitly supplied input types must satisfy all SQL uses of each parameter,
including when the supplied types are outside those inference candidates.
Unresolved expression results are distinct from parameter placeholders. If
resolving them would require a catalog-wide type search, analysis reports
`inference_candidate_space_unsupported` and leaves their types unresolved. This
indicates a limitation of inference, not invalid SQL. Supply SQL type context or
an input type hint to avoid that search; types established by later occurrences
are considered before this diagnostic is emitted. For `bytea`, the protocol's
text value format does not change its SQL type to `text`; use SQL context or an
explicit `bytea` input hint.

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
different conditions. Neither establishes a usable application input type.
