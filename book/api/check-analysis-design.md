# Schema context and query analysis API

This page describes how a client can prepare a schema context and request SQL
analysis from `sql-fun-server`. It covers the API used by `sql-fun check`
and other clients that need parameter descriptions, result-set columns, and
SQL diagnostics without executing a query.

The authenticated server implements schema initialization, query analysis and
session-local paged collections. CLI `check` submits recovered query
requirements; diagnostic presentation and lockfile integration remain unfinished
(#689).

## Before you begin

Connect and authenticate as described in the
[initialize method](./initialize.md) reference. Authentication and schema
initialization are separate operations.

To analyze a query, a client needs:

- a selected datasource and its schema configuration;
- the schema dump and applicable engine/builtin and extension definitions;
- the application SQL query and, optionally, its resolved CTE dependencies.

The server reads explicitly selected local schema files. Client and server
therefore need a shared view of those file paths. A file's availability does
not make it an implicitly selected input.

## Workflow

The call sequence is:

```text
initialize                -> authenticate connection
initialize_schema_context -> receive a context_handle and schema diagnostics
analyze_query             -> send a query with that context_handle
read_collection           -> read remaining results, when a collection is returned
release_collection        -> release collections when no longer needed
```

A client can analyze multiple queries against the same initialized context.
The client must supply the context handle with every analysis request; the
server does not infer a datasource from the SQL text. A client using more
than one datasource initializes and selects their contexts separately.

### 1. Initialize a schema context

Call `initialize_schema_context` with the selected datasource and schema
configuration. The intended operation makes the selected engine's builtin
objects, selected extensions, and datasource schema dump available to
subsequent analysis.

<!-- Method pages are generated in the staged manual build. -->
<!-- rumdl-disable MD057 -->
See the generated [`initialize_schema_context` reference](methods/initialize_schema_context.md)
for the request and response schemas.
<!-- rumdl-enable MD057 -->

Clients transfer configuration using `sql_fun_core::SqlFunArgs::environments`
or the depth-aware `analysis_environments` method.
The server passes this map to core to restore `SqlFunArgs`, then asks core for the
named datasource's metadata and effective configuration. Dialect, engine version,
extensions, search path, and directory overrides follow core's existing argument
and metadata precedence. They are not separately supplied or reconciled by RPC.

The map must explicitly select a nonempty `SQL_FUN_HOME` and
`SQL_FUN_METADATA_FILE`. Core rejects omitted selections before accessing assets;
the server does not read its ambient process environment or discover a project.
Other settings are interpreted by core's existing environment parser; unknown
keys follow that parser's behavior and are ignored. Paths are server-visible;
relative paths resolve from the server working directory, so clients should use
absolute paths. Only the requested datasource's metadata is decoded.

Initialization currently supports PostgreSQL. Extension scripts and their order
are selected through core's configuration resolution, including explicit directory
overrides. The usual extension directory is
`<SQL_FUN_HOME>/postgresql/<engine-version>/extensions`.

The first successful cache service registration fixes the builtin installation
for the server lifetime. A later request selecting a different canonical
`SQL_FUN_HOME` is rejected; restart the server to select a different installation.

A context handle, when provided, is passed unchanged to later analysis
requests. The client should inspect initialization diagnostics before using
the context. A missing handle must not be treated as a successfully initialized
context. Successful initialization returns a handle even when recoverable schema
diagnostics are present. Fatal loading/parsing/analysis failures return a JSON-RPC
error and no usable context handle. Context handles remain valid only in the
connection that initialized them, until that connection closes.

The cache service now supports shared builtin and ordered extension prefixes.
Initialization registers its result in the requesting connection.
The datasource must be explicitly selected; an omitted or unknown selection
is an error. The server reads only the schema files selected for that request.
Client and server must see the same local files. Aliases resolving to the same
canonical path share a file identity; separate canonical paths have separate
identities even when contents match.

Full-project initialization requires an `expected_hash` of the original dump
text, calculated by the client using `sql_fun_core::Hasher::hash_file`. The server
first searches its in-memory schema
cache for that hash under the same parent schema and resolved dump path. A hit
returns the historical schema without reading the dump, even if the dump has
changed or been deleted. This also preserves cached syntax or semantic errors.
Historical entries are available for the server service lifetime; restarting
loses them. Different builtin or extension layers form a different parent schema.

On a cache miss, sqlast reads and hashes the original text before preprocessing.
Cache identity and analysis use the same captured text. If the captured hash
differs from `expected_hash`, initialization fails with a schema-dump-changed
error after caching the captured result. Recalculate the client hash and retry
to select that result.

Extracted builtin JSON masters are fixed for the server lifetime; manually
modifying them is unsupported. Restart after re-extraction. Builtin `schema.sql`,
extension, and application layers observe edits without restarting, using local
filesystem change notifications and file replacement checks;
these are an optimization rather than a guarantee for filesystems with missing
notifications. Schema cache eviction is not yet implemented.

### 2. Submit a query for analysis

<!-- rumdl-disable MD057 -->
Call `analyze_query` with the handle returned by
`initialize_schema_context`. The server analyzes SQL against that schema;
it does **not** execute SQL or connect to the application's database to
fetch query results. The [method reference](methods/analyze_query.md) describes
the request and response schemas.
<!-- rumdl-enable MD057 -->

Always supply `query`. Supply `resolved_cte_catalog` separately when using
resolved CTE dependencies:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "analyze_query",
  "params": {
    "context_handle": "context-0",
    "query": {"Text": "SELECT ${id:int4} AS id; SELECT 2 AS next"}
  }
}
```

`query` accepts `Text` or `File` (an explicitly selected server-local path).
`Url` input returns `InvalidAnalysisInput` without fetching. A resolved CTE
catalog uses the shared `sql-fun-core::ResolvedCteCatalog` JSON representation
and contains dependency definitions, aliases, and aggregated CTE parameter
metadata. It does not contain the application query. `query` remains required
and authoritative with or without a catalog; missing or null SQL input is an
invalid request. The catalog may be omitted or null for a plain query.

The server never loads CTE catalog files. CLI `check` sends recovered SQL as
`query.Text` and its resolved dependencies as `resolved_cte_catalog`, including
empty catalogs for queries without referenced CTEs. Both text and explicitly
selected file inputs can be combined with resolved dependencies.

Without referenced CTE expansion, multiple SQL statements are accepted and
analyzed in input order, including a catalog with zero referenced CTEs. The
caller does not split SQL. With referenced CTE expansion, the base SQL must
contain exactly one statement; otherwise the result contains a realization
diagnostic. Empty direct or zero-reference input returns an empty query
collection. This does not introduce a separate batch protocol.

Named `${name}` placeholders retain their identity across repeated occurrences;
`${name:int4}` supplies an explicit SQL parameter type. Catalog CTE parameters
precede base-query parameters in dependency and first-occurrence order. The
same numbered binding order is used for hint application, analysis and returned
SQL. Native numbered parameters retain their actual slots, including gaps.
Use one placeholder style per input; named and native numbered parameters must
not be combined because their slots can collide.

Hints apply only to the current query's semantic construction and do not insert
SQL casts. Unhinted inputs use SQLAST inference. Hints use the existing type-name
resolver, including its documented syntax and type-modifier limitations; see
[parameter analysis](../sql-analysis/expressions/parameters.md). No Rust type
inference is performed by this API.

Set `analyze_access: true` on an `analyze_query` request to collect SQL-visible
relation and column accesses. Omission and `false` disable this additional
analysis. Existing request constructors leave it disabled; typed clients can
call `AnalyzeQueryArgs::with_access_analysis()` to enable it. The same option
applies when `resolved_cte_catalog` is supplied.

### 3. Read the analysis result

`result.queries` is the existing `Collection<AnalyzeQueryItem>`. Each item has:

| Field | Meaning |
| --- | --- |
| `query_span` | Original base-input UTF-8 byte range, when known; line/column are one-based |
| `query_kind` | SELECT, INSERT, UPDATE, DELETE, MERGE, TRUNCATE or Other; absent when unavailable |
| `execute_sql` | Executable SQL, including explicitly enabled datasource rewrites; absent when unavailable |
| `complete` | Analysis finished without errors or unresolved parameter types |
| `probrems` | Structured diagnostics (the existing wire spelling is retained) |
| `parameters` | Binding names, actual one-based `parameter_index`, optional finalized `parameter_type`, base-input spans and nullability |
| `result_columns` | Zero-based ordinal, name, optional `column_type` and three-state nullability |
| `column_access` | SQL-visible access records when explicitly requested; otherwise empty |

Each `column_access` record has exactly four semantic fields:

```json
{
  "access_kind": "SelectOutput",
  "relation_kind": "Table",
  "relation": {"catalog": null, "schema": "public", "name": "entries"},
  "column_name": "id"
}
```

`relation` preserves resolved logical identifier components, including quoted
case and embedded punctuation. It is not a dotted string to split. `relation_kind`
is `Table`, `View`, `MaterializedView`, or `Function`. A null `column_name` denotes
a relation-level action; a string denotes a resolved column. Both are returned in
this single collection, with identical records deduplicated and distinct
purposes retained.
The collection uses the same paging, ownership and release protocol as other
query collections.

| Top-level purpose | `access_kind` values |
| --- | --- |
| Relation actions | `InsertRows`, `ReadRows`, `UpdateRows`, `DeleteRows` |
| SELECT output, source expressions, filtering, joins | `SelectOutput`, `SelectSource`, `SelectFilter`, `SelectJoin` |
| SELECT grouping, HAVING, ordering, DISTINCT | `SelectGrouping`, `SelectGroupFilter`, `SelectOrdering`, `SelectDistinct` |
| Named SELECT windows, LIMIT, OFFSET | `SelectWindow`, `SelectLimit`, `SelectOffset` |
| INSERT destination, input, RETURNING, conflict selection | `InsertTarget`, `InsertInput`, `InsertReturning`, `InsertConflict` |
| UPDATE destination, input, sources, filtering, joins, RETURNING | `UpdateTarget`, `UpdateInput`, `UpdateSource`, `UpdateFilter`, `UpdateJoin`, `UpdateReturning` |
| DELETE sources, filtering, joins, RETURNING | `DeleteSource`, `DeleteFilter`, `DeleteJoin`, `DeleteReturning` |
| MERGE sources, matching, action conditions, RETURNING | `MergeSource`, `MergeJoin`, `MergeCondition`, `MergeReturning` |

MERGE action destinations and values use the corresponding INSERT/UPDATE kinds;
its relation actions reflect the actions present in the statement. `InsertRows`
means row insertion, not schema-object creation. A source read exists even in
`SELECT 1 FROM entries`; DELETE does not manufacture column writes.

A purpose describes why the **top-level statement** needs a column. Nested
expressions, aggregate/window modifiers and subqueries retain the enclosing
purpose. For `SELECT (SELECT b.value FROM b WHERE b.id = a.id) FROM a`, all
three column references have `SelectOutput`, including the inner WHERE and
correlated `a.id`. The same nesting in an outer WHERE uses `SelectFilter`.
Multiple uses in different top-level clauses retain separate records.

Aliases, CTEs and derived tables refer back to their resolved inputs within the
submitted SQL. View columns belong to the SQL-visible view; table-function outputs
belong to the resolved function. Neither view definitions nor function bodies
are expanded. Function argument expressions in the submitted SQL are analyzed.
The records describe static SQL references, including clauses that might never execute.

An unrequested analysis and a requested query with no accesses both return an
empty collection. Interpret that using your request choice. Accesses do not
use `complete`, executable SQL availability or Rust parameter eligibility as a
completeness indicator. Existing semantic errors remain ordinary diagnostics;
unresolved names do not acquire invented object identities.

The `parameter_index` is the SQL `$n` identity, not a collection ordinal. For
`SELECT $3::int4, $1::int4, $3::int4`, the binding metadata describes slots 1 and
3; repeated use does not create a new binding. `execute_sql` and these indexes
supply the missing data for the caller's `LockfileEntry`. The originating caller
retains `query_name` and `query_finger_print`; they are not echoed.

Only definite named SQL types are returned as typed parameter metadata, with
array identity preserved by `is_array`. Unresolved parameter inference is a
SQLAST diagnostic, not a candidate list or a server-side inference solver.
Non-singleton and unrestricted input domains produce the error
`parameter_type_unresolved` and `complete: false`: the application's binding
contract cannot be finalized. This does not imply the SQL is intrinsically
invalid or that PostgreSQL cannot prepare it. A known result-column type can
still be returned independently of the missing input type. Explicit input hints
such as `${src:text}` can finalize a binding where `${src}::text` only constrains
the result of a cast. See [parameter analysis](../sql-analysis/expressions/parameters.md).
Contradictory constraints retain their SQLAST diagnostics.
A result-column `column_type: null` is preserved without inventing a type or
adding an error solely for its absence. Anonymous dynamic types currently have
no named API representation. Nullability is `NotNull`, `Nullable` or `Unknown`.
The CLI/lockfile consumer decides whether these results suffice for its task;
`execute_sql` availability alone does not establish fully typed analysis.

For PostgreSQL datasources with `rewrite_options.expand_asterisk = true`,
`execute_sql` contains explicit projections for table-dependent wildcards. This
preserves the analyzed column names and order; unrelated stars remain unchanged.
See
[wildcard projection expansion](../sql-analysis/statements/select.md#wildcard-projection-expansion)
for configuration scope and schema limitations.

SQLAST diagnostics retain their original structured problem. Direct-input
presentations retain the analyzed, numbered statement and its offset. For
expanded CTEs, individual AST locations cannot reliably identify one combined
source: diagnostics instead retain `semantic_problem` without inventing a
presentation or location. Parameter spans describe occurrences in the original
base input, not locations in separately supplied CTE definitions.

Statement classification is independent of semantic support. `Other` classifies
valid statements outside the named execution kinds, including DDL; it does not
mean invalid SQL. Statements without result columns can complete successfully.
Analysis does not execute DDL or retain its schema changes for subsequent statements.
MERGE uses the existing analyzer, including RETURNING. TRUNCATE retains its
classification even though common-AST conversion is currently unavailable.

Existing SQLAST `todo!()` paths remain explicitly unimplemented, including
special operators (#513), TABLESAMPLE, ON CONFLICT DO UPDATE (#474), and some
CASE common-type selections. The RPC neither catches these panics nor converts
them into ordinary diagnostics or unknown metadata. PostgreSQL is the first
end-to-end target; SQLite schema initialization is not yet available here.

### 4. Display diagnostics

A valid API request can contain invalid SQL. In that case, the
response carries SQL diagnostics instead of treating the syntax or semantic
error as a transport failure.

For example:

```sql
SELECT missing_column FROM users;
```

If `users` exists but `missing_column` does not, the client should receive
a diagnostic identifying the unresolved column, together with the location
available from analysis. Any independently recoverable result information
may still be returned.

The intended diagnostic information includes:

- a machine-readable code and a human-readable message;
- severity (such as error or warning);
- the source to which a diagnostic applies;
- a primary location and, where applicable, related locations.

When a reusable CTE was expanded, an error may refer to the original base
query, a CTE definition, or generated SQL. Clients must not assume every
span is an offset into the same SQL string. If an exact original location
cannot be recovered, it must not be fabricated.

The `AnalysisProbrem` type carries `code`, `severity`, `file_path`,
`statement_span`, `probrem_span`, and `statement_relative_span`. Severity is
`error`, `warning`, or `advice`. Clients should use it to distinguish rejected
SQL from accepted SQL with a warning, such as a statistics target clamped to
10000. `presentation` carries the retained statement and structured diagnostic
for CLI rendering. `semantic_problem` preserves a diagnostic when a reliable
source presentation is unavailable.
Schema diagnostic codes identify the retained problem kind. Diagnostics keep
layer order (builtin, ordered extensions, application dump) and analysis order
within each layer. A missing `statement_span` or `probrem_span` is `null`.
The cache does not retain statement positions in the file, so schema diagnostics
return `null` for both file spans. If analysis retained a range inside its SQL
statement, `statement_relative_span` contains its byte `offset` and `length`.
This range cannot be treated as an offset into the schema file. The server does
not reopen or reparse source files to invent missing positions.

A JSON-RPC error represents a problem with the request, context state, or
service operation. SQL diagnostics are analysis results. For schema
initialization, loading failures and expected-hash mismatches use
`SchemaContextInitializeFailed`. The error envelope includes a readable message
and its serialized API error in `data`. Query analysis uses
`InvalidContextHandle` for an unknown or foreign context and
`InvalidAnalysisInput` for missing, conflicting, or unsupported input sources.
An unreadable explicitly selected file is a service operation error. SQL parse,
realization, and analysis failures remain diagnostics in the query result.

### 5. Read and release result collections

<!-- rumdl-disable MD057 -->
Some result fields are represented as `Collection<T>`. When a response
provides a collection handle, the intended client flow is to use
[`read_collection`](methods/read_collection.md) for remaining items and
[`release_collection`](methods/release_collection.md) when done.
<!-- rumdl-enable MD057 -->

A collection belongs to the requesting connection's session. Do not pass a
collection handle to another connection. `read_collection` accepts
`collection_handle`, `start_offset` and `count`. Reads are repeatable at arbitrary
offsets and preserve the original element type. Every page, including the initial
diagnostic page, contains at most 1,024 items. `count: null` requests the remaining
items subject to that limit. Zero counts, exhausted offsets, and offsets beyond
the end return empty pages with the original `total_length` and requested
`offset`.

Retained collections set `require_release: true`, including empty collections
and their final pages. They remain registered until `release_collection` or
connection teardown; there is no expiration or automatic release on exhaustion.
Release returns JSON `null`. Unknown, foreign-session, and already released
handles produce the same `InvalidCollectionHandle` API error; repeated release
is an error. Such RPC errors leave the connection usable.

Each `sql-fun api` invocation creates a new session. A handle from one invocation
cannot be continued or released by another invocation. Use a persistent RPC
connection when continuation is needed.

## Using the API from `sql-fun check`

The CLI extracts query requirements from expanded Rust source. Each requirement
contains its datasource, query, `ResolvedCteCatalog`, and Rust source
location. It initializes a context per referenced datasource on one persistent
connection and submits each recovered catalog with the corresponding context
handle. It materializes and releases response collections while retaining source
identity. Catalog definitions, aliases and parameter bindings are transported
unchanged; the CLI does not send a second copy of the base SQL.

The CLI consumes correlated responses to regenerate or validate lockfiles from
this session's eligible query functions. It preserves statement order and
per-statement bindings and result shapes for multi-statement functions. It waits
for successful submission and connection completion before saving any output.
See [`check`](../cli/check.md) for validation, empty-session, and progress behavior.
The inventory-producing request explicitly enables `analyze_access`. The CLI
aggregates the returned single access collection independently of `complete`,
parameter inference and lockfile eligibility. It does not classify SQL diagnostics
to infer access-analysis status. Relation/column groups preserve detailed access
kinds and reference source-correlated queries and their statement indices in
`sql-fun.<datasource>.<schema>.toml`, alongside the existing lockfile.

The server handles SQL analysis and reusable CTE expansion. The CLI handles
Rust source extraction, source-level reporting, lockfile policy, and cross-query
inventory aggregation/rendering. The
server's analysis of SQL parameters does not itself establish whether a Rust
function's declared parameter types are compatible.

## Schema prefix selection

`initialize_schema_context` accepts optional `schema_level`: `builtin`, `default`,
`extensions`, or `project`. Omission preserves full-project behavior for existing
clients. Each value returns a completed prefix of the existing layered schema
cache. `builtin` loads catalog definitions only, `default` adds bundled
`schema.sql`, `extensions` adds configured extensions in order, and `project`
adds the selected project dump. Excluded SQL inputs are not resolved or read.
`schema_dump` and `expected_hash` may be omitted below `project` and are validated
only for `project`; lower-prefix responses have a null `schema_dump_hash`.
The returned context handle works with the existing analysis API at every level.
