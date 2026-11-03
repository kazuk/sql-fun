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
several admissible types. These domains describe alternatives for explicitly
typed inputs, rather than PostgreSQL's single signature inferred by an untyped
`PREPARE`. For example, `SELECT ARRAY[$1,1]` admits an `integer` input with an
`integer[]` result and a `bigint` input with a `bigint[]` result. PostgreSQL
chooses `integer` when preparing that statement without parameter types.
`ParameterConstraints::allows` checks admissible assignments; the internal
SQLAST analysis retains alternatives without selecting a final protocol binding.
A result column's `col_type()` is absent when several result types remain;
inspect `ParameterConstraints::result_column` for its domain. An explicit SQL
cast can narrow the result type.

The public `analyze_query` API must finalize an application's input binding
contract. If SQL inference or a supported explicit input type hint does not
establish one input type, it returns `parameter_type_unresolved` as an error and
marks the statement incomplete. This applies to non-singleton and unrestricted
input domains. An empty domain instead retains its conflict diagnostics: no
input assignment satisfies those requirements.

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
`ParameterConstraints::parameter(...).constraint()` exposes the current
inference candidates. `ParameterConstraints::allows` checks explicitly supplied
types against the SQL constraints, including types outside those candidates.
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

### SQLAST API callers

The following setup and inspection contracts are for Rust callers using SQLAST
directly; CLI users should follow the [analyze command](../../cli/analyze.md),
and RPC clients the [query analysis API](../../api/check-analysis-design.md).

SQLAST API callers can install explicit SQL input types on a mutable context
before calling `Statement::analyze`. Import `ParameterTypeHintWrite` and call
`context.setup_typehinting(&[Some("pg_catalog.int4"), None])`: the first slot
supplies the type of `$1`, and the second leaves `$2` for normal inference.
`ParameterTypeHintRead::get_parameter_typehint` takes the SQL parameter number
unchanged; zero and numbers outside the supplied slice return `None`. PostgreSQL
`unknown` (including its qualified spelling) leaves the slot for normal
inference, like `None`.

Type hints accept SQL type names for the context's explicitly selected dialect,
then resolve them using its catalog and search path. PostgreSQL examples include
`INTEGER`, `double precision`, `character varying`, `timestamp with time zone`,
`pg_catalog.int4`, quoted names, and arrays such as `integer[2][]` or
`integer ARRAY`. Quoted identifiers preserve case; unquoted identifiers follow
SQL case folding. `float(24)` selects `real`, and `float(53)` selects
`double precision`. Bare `character` and `bit` select their type identities
without imposing column-declaration length constraints. SQLite accepts its
quoted and multiword declared type names, including backticks and brackets;
PostgreSQL aliases are not applied to SQLite names. Empty `[]` suffixes retain
the existing SQLite/Turso array type identity, while `[name]` quotes a name.

Parameter types describe client-protocol type identities. PostgreSQL parameter
OIDs carry no type modifiers, so declarations such as `varchar(100)`,
`numeric(10,2)`, `timestamp(2) with time zone`, and `interval day to second` are
rejected rather than silently losing their constraints. Declaration size syntax
is also rejected for SQLite hints.

Callers that only need to interpret spelling can use
`parse_client_type(input, &context)` from `sql_fun_sqlast::sem`. This shared API
returns a `TypeReference` without catalog lookup or search-path resolution;
syntactically valid user-defined names need not exist yet. Canonical
`FullName::from_str` and `TypeReference::from_str` retain their separate,
case-preserving spelling and serialization contracts.

Malformed or unresolved names return an error and preserve the complete previous
setup. Successful setup replaces all previous hints, including when passed an
empty slice or only `None` entries.

Create an independent `sql_fun_sqlast::context::SchemaFileContext` over the
shared `sql_fun_sqlast::context::FrozenBaseContext` catalog for each query. If
reusing a mutable context, explicitly install the next query's hints, including
an empty setup for unhinted queries. Nested queries and realized CTEs share that
query's finalized parameter order and hints. Hints assign SQL types during name
resolution; incompatible uses still produce semantic diagnostics. They do not
insert casts or change executable SQL. The accepted spelling is independent of
the named-placeholder lexer's current `${name:ident}` hint syntax.

`ParameterConstraints::allows` checks complete assignments and preserves
relationships between positions; checking each parameter's domain independently
can admit an incompatible combination. Omitted positions remain unconstrained by
the inspection; unknown parameter positions are rejected. Inspection does not
install hints, choose an application binding or change executable SQL.

## Dialect and engine-specific behavior

PostgreSQL implicit-cast and overload rules above do not become SQLite typing
rules. Native SQLite numbered parameters (`?1`) participate in supported syntax,
but operator-dependent inference and parameter-dependent CASE/COALESCE result
typing remain incomplete. SQLite casts use affinity semantics, not PostgreSQL
cast declarations. SQLite SQLAST availability requires the `sqlite` feature; it
does not establish SQLite availability through every server/CLI workflow.

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

An empty domain means a conflict; an unrestricted domain means no specific type
requirement. Neither is silently converted into a successful input binding.
