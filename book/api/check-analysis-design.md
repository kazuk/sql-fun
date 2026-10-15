# Schema context and query analysis API

This page describes how a client can prepare a schema context and request SQL
analysis from `sql-fun-server`. It covers the intended API for `sql-fun check`
and other clients that need parameter descriptions, result-set columns, and
SQL diagnostics without executing a query.

> **API design draft:** The workflow and response capabilities below describe
> the intended contract, not a fully available server feature. The server
> implements connection-local `initialize_schema_context`, `read_collection`,
> and `release_collection` after authentication. `analyze_query` is declared in
> `sql-fun-server-api` but is not yet dispatched by the JSON-RPC server.
> Examples marked **proposed** are illustrative, not executable requests.

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

The intended call sequence is:

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
| `engine_dialect` | SQL syntax dialect |
| `engine_version` | Engine builtin definitions to use |
| `extensions` | Selected extension names and versions |
| `search_path` | Name-resolution search path |
| `schema_dump` | Server-visible path to the schema dump |
| `expected_hash` | Required lowercase hexadecimal SHA-256 of the original dump text |
| `environments` | Explicit `SQL_FUN_HOME` and `SQL_FUN_METADATA_FILE` selections |

Both environment entries are required and must be nonempty. Other environment
keys are rejected; the server does not inherit its process environment to resolve
request settings or discover a project. Paths are server-visible; relative paths
resolve from the server working directory, so clients should use absolute paths.

The selected metadata must declare PostgreSQL and the same dialect as the request.
If metadata also declares a version, search path, or extension list, it must agree
with the corresponding explicit request setting (including extension order).
Conflicts are errors. Missing optional metadata settings use the explicit request
values. Every extension must include its selected name and installation version;
the supplied extension order is the load order. Engine versions must be nonempty
installation names, not paths. Extension names and versions cannot contain the
CSV separators `,` or `@`. Extension installation scripts are resolved using
the existing PostgreSQL extension directory:
`<SQL_FUN_HOME>/postgresql/<engine-version>/extensions/<name>--<extension-version>.sql`.
Initialization reads only the named installation scripts and does not infer
extension dependencies or choose a version.

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

For `sql-fun check`, a query may refer to reusable CTE definitions. The
client can supply the shared `sql-fun-core::ResolvedCteCatalog` value, which
already contains the base query and its resolved CTE definitions. The server
is responsible for realizing those definitions into the query.

**Proposed logical input for the `sql-fun check` use case** (field layout is
not yet the wire schema):

```text
analyze_query
  context_handle: ContextHandle
  query: ResolvedCteCatalog
  query_identity: optional client correlation identifier
```

The client does not need to submit a second copy of the SQL alongside that
catalog. If a future API also supports direct SQL input, direct SQL and a
resolved CTE catalog will be alternative input forms rather than two sources
that must agree.

The **currently declared** `AnalyzeQueryArgs` instead accepts a
`context_handle` and `AnalyzeQueryInput` (`Text`, `File`, or `Url`).
That existing declaration does not yet accept the resolved catalog required
by this workflow.

For a query without reusable CTEs, the caller should still use an explicitly
supported input form; do not assume the server will discover or attach CTEs.

### 3. Read the analysis result

The intended `analyze_query` result describes the query, not a set of rows
returned from a database.

| Result | What the client can use it for |
| --- | --- |
| Query identity and statement kind | Associate results with the submitted query and classify the statement |
| Realized executable SQL | Generate or validate the SQL recorded in the lockfile |
| Bind parameter descriptions | Map placeholder positions to parameter names, inferred SQL type constraints, and nullability |
| Result-set columns | Determine ordered output column names, SQL types, and nullability |
| Accessed relations and columns | Identify schema dependencies for inventory and validation |
| Diagnostics | Report SQL syntax/semantic errors and warnings at their available source locations |
| Analysis completeness | Recognize when an error prevented some fields from being inferred |

These are **target response capabilities**. They are not all represented by
today's `AnalyzeQueryResponse` or `AnalyzeQueryItem` types.

In the existing type declarations, `AnalyzeQueryResponse` contains a
`Collection<AnalyzeQueryItem>`. Each item has `query_span`,
`query_kind`, `probrems`, `accessing_relations`, `column_access`,
`parameters`, and `result_columns`. The fields for executable SQL and
analysis completeness are not present yet.

#### Bind parameters

A parameter description should tell the caller how to bind a value and
where a parameter occurs. The existing `ParameterDescribe` includes
`name`, `spans`, `type_candidate`, and `nullability`.

For a query using reusable CTEs, **binding order is significant**: parameters
from catalog CTEs appear first in their placeholder-token order, followed
by parameters from the complete base query in its placeholder-token order.
The server must return the definitive order so that the client can produce
`LockfileEntry.bind_parameters` consistently. Callers must not alphabetize
parameters or derive order from parameter names.

SQL dialects can have distinct placeholder forms and type semantics.
Inferred type alternatives, unknown types, and contradictory constraints
must be distinguishable; a client should not interpret an empty type-candidate
list as an unambiguous type.

#### Result-set columns

Result columns are ordered by their ordinal. The current
`ResultSetColumn` declaration includes `ordinal`, `name`,
`column_type: Option<TypeReference>`, and `nullability`.

The eventual response needs to represent dialect-specific type information,
including cases where more than one type is possible. A missing type must
not be reported as a concrete SQL type.

#### Referenced schema objects

Relation and column access information can be used to track dependencies
and explain unresolved references. If a reference cannot be resolved,
clients should use the diagnostic rather than assume the response contains
a valid object identity. The fidelity of source-object attribution through
CTEs, views, joins, and derived expressions remains subject to the semantic
analysis capabilities.

### 4. Display diagnostics

A valid API request can contain invalid SQL. In that case, the intended
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

The `AnalysisProbrem` type carries `code`, `file_path`, `statement_span`,
`probrem_span`, and `statement_relative_span`. Message, severity, and richer source
identity are intended capabilities that are not yet part of that type.
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
and its serialized API error in `data`. Other query-analysis error details remain
under design.

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
location. The intended integration is:

1. Initialize a context for each explicitly selected datasource used by the
   extracted queries.
2. Submit each resolved catalog with the corresponding context handle.
3. Read diagnostics, definitive bind order, inferred types, result columns,
   and referenced schema objects.
4. Associate diagnostics with the originating Rust query and use successful
   analysis for lockfile validation or generation.

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
| `sql-fun-server-api/src/analyze_query.rs` | Historical query input and result shapes |
| `sql-fun-sqlast` | `FrozenBaseContext`, layered schema loading, recoverable schema problems |
| `sql-fun-core` | Shared `ResolvedCteCatalog` and its serialization |
| `sql-fun-codegen` | `QueryRequirements::extract` |
| `sql-fun-server/src/cte_realization.rs` | `realize_cte_catalog` produces a common AST and `OrderedBindings`; it is server-internal |
| `sql-fun-server/src/lib.rs` | Connection-local initialization and collection RPCs implemented; query-analysis and object-consumer handlers remain unfinished |
| `cli/src/check.rs` | Requirements extraction exists; server analysis and lockfile integration are incomplete |

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
