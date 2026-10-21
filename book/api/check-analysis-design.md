# Schema context and query analysis API

This page describes how a client can prepare a schema context and request SQL
analysis from `sql-fun-server`. It covers the intended API for `sql-fun check`
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
- the SQL query or a resolved CTE catalog representing the query.

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

The **initialization request** (`InitializeSchemaArgs`) contains:

| Field | Meaning |
| --- | --- |
| `datasource` | Datasource to initialize |
| `schema_dump` | Server-visible path to the schema dump |
| `expected_hash` | Required lowercase hexadecimal SHA-256 of the original dump text |
| `environments` | Configuration map from `sql_fun_core::SqlFunArgs::environments` |

Clients transfer configuration using `sql_fun_core::SqlFunArgs::environments`.
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

The **currently declared response** (`SchemaContext`) contains:

| Field | Meaning |
| --- | --- |
| `context_handle` | Optional opaque handle to a usable schema context |
| `schema_dump_hash` | SHA-256 of the schema dump (not a fingerprint of all analysis settings) |
| `problems` | Collection of diagnostics from schema loading |

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

Initialization requires an `expected_hash` of the original schema-dump file
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

Call `analyze_query` with the handle returned by
`initialize_schema_context`. The server analyzes SQL against that schema;
it does **not** execute SQL or connect to the application's database to
fetch query results.

Supply exactly one of `query` and `resolved_cte_catalog`:

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
catalog uses the shared `sql-fun-core::ResolvedCteCatalog` JSON representation;
its base query is authoritative, so `query` must be omitted. The server never
loads CTE catalog files. CLI `check` submits the recovered catalog directly to
this API, including catalogs without referenced CTEs.

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
[parameter analysis](../sql-analysis.md#parameter-analysis). No Rust type
inference is performed by this API.

### 3. Read the analysis result

`result.queries` is the existing `Collection<AnalyzeQueryItem>`. Each item has:

| Field | Meaning |
| --- | --- |
| `query_span` | Original base-input UTF-8 byte range, when known; line/column are one-based |
| `query_kind` | SELECT, INSERT, UPDATE, DELETE, MERGE, TRUNCATE or Other; absent when unavailable |
| `execute_sql` | SQL deparsed from the realized AST; absent when unavailable |
| `complete` | Analysis finished without errors or unresolved parameter types |
| `probrems` | Structured diagnostics (the existing wire spelling is retained) |
| `parameters` | Binding names, actual one-based `parameter_index`, optional finalized `parameter_type`, base-input spans and nullability |
| `result_columns` | Zero-based ordinal, name, optional `column_type` and three-state nullability |
| `accessing_relations` | Available resolved relation identities |
| `column_access` | Reserved collection; detailed CRUD/provenance is separate work (#425) |

The `parameter_index` is the SQL `$n` identity, not a collection ordinal. For
`SELECT $3::int4, $1::int4, $3::int4`, the binding metadata describes slots 1 and
3; repeated use does not create a new binding. `execute_sql` and these indexes
supply the missing data for the caller's `LockfileEntry`. The originating caller
retains `query_name` and `query_finger_print`; they are not echoed.

Only definite named SQL types are returned as typed parameter metadata, with
array identity preserved by `is_array`. Unresolved parameter inference is a
SQLAST diagnostic, not a candidate list or a server-side inference solver.
Contradictory constraints retain their SQLAST diagnostics.
A result-column `column_type: null` is preserved without inventing a type or
adding an error solely for its absence. Anonymous dynamic types currently have
no named API representation. Nullability is `NotNull`, `Nullable` or `Unknown`.
The CLI/lockfile consumer decides whether these results suffice for its task;
`execute_sql` availability alone does not establish fully typed analysis.

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

Some result fields are represented as `Collection<T>`. When a response
provides a collection handle, the intended client flow is to use
`read_collection` for remaining items and `release_collection` when done.

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

The remaining consumer integration (#689) will display diagnostics, use the
returned definitive bind order and result shape, and validate or generate
lockfile entries for successful analyses. Until that integration is complete,
`check` still fails at its unimplemented completion step after submission.

The server handles SQL analysis and reusable CTE expansion. The CLI handles
Rust source extraction, source-level reporting, and lockfile policy. The
server's analysis of SQL parameters does not itself establish whether a Rust
function's declared parameter types are compatible.

## Implementation notes (temporary; remove when the API is implemented)

The remainder of this page records pending design and implementation work.
It is not part of the caller-facing API contract and should be removed once
implementation and reference documentation cover the behavior.

### Existing building blocks

| Component | Present state |
| --- | --- |
| `sql-fun-server-api/src/initialize_schema.rs` | Implemented initialization request/response; PostgreSQL-only dialect/version enums |
| `sql-fun-server-api/src/payloads/analyze_query.rs` | Query input and result shapes |
| `sql-fun-sqlast` | `FrozenBaseContext`, layered schema loading, recoverable schema problems |
| `sql-fun-core` | Shared `ResolvedCteCatalog` and its serialization |
| `sql-fun-codegen` | `QueryRequirements::extract` |
| `sql-fun-server/src/cte_realization.rs` | `realize_cte_catalog` produces a common AST and `OrderedBindings`; it is server-internal |
| `sql-fun-server/src/lib.rs` | Connection-local initialization, query analysis and collection RPCs implemented; object-consumer handlers remain unfinished |
| `cli/src/check.rs` | Requirements extraction and server submission exist; diagnostic presentation and lockfile integration are incomplete |

### Pending design and implementation decisions

1. **Additional dialects:** Schema initialization currently supports PostgreSQL.
   Any additional dialect needs its own implemented loading contract.
2. **Context identity:** Decide whether callers need a fingerprint of the
   *entire* semantic context in addition to `schema_dump_hash`.
3. **Analysis inputs:** Reconcile the current `Text/File/Url` variant with
   `ResolvedCteCatalog`, avoiding duplicate base SQL. Define whether
   remote URL fetching is supported and its explicit authorization rules.
4. **Analysis outputs:** Connect the realized common AST to semantic analysis
   and executable SQL generation; identify available nullability, type
   alternatives, provenance, and recovery information. Do not rule out legal
   PostgreSQL or SQLite syntax merely because it is unusual.
5. **Binding contract:** Preserve the existing server `OrderedBindings`
   semantics throughout analysis, response serialization, and lockfile
   generation. Check that all placeholder variants are representable.
6. **Diagnostics:** Define source identity, positional mapping for base-query
   and catalog-CTE spans, severity/message, and how fatal `AnalysisError`
   differs from recoverable `AnalysisProblem` in RPC responses.
7. **Feature consumers:** Connect query analysis and object enumeration to the
   existing connection-local context and collection infrastructure.
8. **End-to-end checks:** Cover schema load errors, unknown datasource/handle,
   missing tables/columns, CTE binding order, type alternatives, multiple
   datasources and concurrent connections, and PostgreSQL/SQLite behavior.

Implementation should update the caller-facing sections to the **actual**
wire format and remove this temporary section rather than leaving draft
internals in the published manual.
