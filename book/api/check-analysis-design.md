# Schema context initialization and query analysis API design

> **Status: design draft, not an implemented API contract.** The current
> JSON-RPC server only dispatches `initialize_schema_context`, and that
> implementation is incomplete. The request and response types in
> `sql-fun-server-api` are earlier sketches. Examples under "Proposed contract"
> illustrate design intent, **not** requests accepted by the current server.

## Purpose and boundaries

Provide the schema-aware server operations needed by `sql-fun check` after
`QueryRequirements::extract` reads unresolved SQL queries from expanded Rust
sources. The server must interpret those queries against a selected datasource
schema, report bindings and result columns, and retain useful diagnostics for
invalid SQL (including references to missing tables or columns).

This is a server API design, not a proposal to move the Rust source reader,
CTE catalog format, or SQL AST ownership between crates.

- `sql-fun-core` owns `ResolvedCteCatalog` and its serialized representation.
- `sql-fun-codegen` extracts `QueryRequirement`, including the datasource,
  base SQL, resolved CTE catalog, and original Rust source location.
- `sql-fun-server` owns CTE realization, schema-context lifetime, semantic
  analysis, and analysis-result collections.
- `sql-fun-sqlast` owns the SQL parser, common AST mutations, layered schema
  catalogs, and semantic analysis.
- The CLI associates server diagnostics with originating Rust source locations,
  checks lockfile consistency, and decides command exit status.

The schema context must be explicitly selected. A reachable metadata file or
available default datasource is not implicit permission to select it or inspect
unrelated project files.

## Existing implementation inventory

| Component | Current status | Evidence |
| --- | --- | --- |
| API shapes | `InitializeSchemaArgs`, `SchemaContext`, `AnalyzeQueryArgs`, `AnalyzeQueryItem`, `ParameterDescribe`, and `ResultSetColumn` exist | `sql-fun-server-api/src/{initialize_schema,analyze_query}.rs` |
| Schema layers | `FrozenBaseContext` is shareable; dialect-specific `parse_schema_file(path, &base)` returns `ParsedSchemaDump`, including recoverable problems | `sql-fun-sqlast/src/{lib,sqlite}.rs`, `sem/frozen_base_context.rs` |
| Schema server initialization | Constructs `SqlFunArgs`, resolves metadata, parses a PostgreSQL builtin file, then reaches `todo!()` | `sql-fun-server/src/lib.rs` |
| CTE realization | `realize_cte_catalog(&ResolvedCteCatalog, Dialect)` already parses CTE and base statements, structurally prepends catalog CTEs, and returns an AST plus `OrderedBindings` | `sql-fun-server/src/cte_realization.rs` |
| Query input discovery | `QueryRequirements::extract` returns each query and deserialized `ResolvedCteCatalog` | `sql-fun-codegen/src/query_requirements.rs` |
| Query semantic analysis | Server `analyze_query` is `todo!()`; no JSON-RPC dispatcher arm exists | `sql-fun-server/src/lib.rs` |
| Collections | `read_collection` and `release_collection` are declared but not implemented | `sql-fun-server/src/lib.rs` |
| CLI integration | `check` extracts requirements but has not connected them to server analysis and lockfile processing | `cli/src/check.rs` |

The API's current `EngineDialect` and `EngineVersion` only enumerate
PostgreSQL. That must not be taken as a decision to exclude SQLite.
`AnalyzeQueryInput` currently supports `Text`, `File`, and `Url`, but it
cannot represent the fully resolved CTE input required by `check`.

## Schema initialization

### Target data flow

```text
Explicit datasource and schema configuration
    |
    v
BaseContext (selected dialect / engine builtin metadata)
    |
    v
PostgreSQL builtin schema (where applicable)
    | freeze
    v
Selected extension schema layer(s), in resolved dependency order
    | freeze after each successful layer
    v
Selected datasource schema dump
    | freeze
    v
Immutable, shareable schema context
    |
    v
ContextHandle
```

The layered sqlast model supports loading an ordered file over a frozen base
without mutating that base or its siblings. This does **not** establish that
extension file discovery, dependency ordering, or all dialect-specific loading
are already available in the server.

### Proposed contract

`initialize_schema_context` resolves explicitly selected settings, loads the
ordered schema layers, and returns an opaque handle only for a usable final
context. A query must identify that handle; the server must not choose another
datasource on an unknown handle.

The existing request contains `datasource`, `engine_dialect`,
`engine_version`, `extensions`, `search_path`, `schema_dump`, and
`environments`. Its earlier shape mixes metadata resolution with explicit
overrides. Before revising it, specify for each setting whether the source of
truth is selected datasource metadata or the request, and how conflicts fail.
Do not silently select one source or infer missing options.

The current `SchemaContext` fields are:

- `context_handle: Option<ContextHandle>`
- `schema_dump_hash: String`
- `problems: Collection<AnalysisProbrem>`

Preserve the distinction between an error executing the initialization
operation and recoverable schema diagnostics. Specify whether a partially
loaded schema may ever produce a usable handle. The sqlast guarantee that
previous successful layers remain usable after a later fatal failure is *not*
a server promise to publish that partial state.

`schema_dump_hash` identifies only the dump bytes. If the server or lockfile
needs an identity for analysis equivalence, define a separate context
fingerprint incorporating all semantically relevant inputs (builtin version,
extensions and order, search path, schema dump, dialect, etc.). Do not rename
the dump hash to imply such a guarantee.

### Missing implementation and decisions

1. Complete loading of every applicable selected layer: builtin, ordered
   extensions, and datasource dump; retain recoverable layer diagnostics.
2. Verify how extension definitions and versions are located, and whether
   their dependency order is already available from the selected metadata.
3. Freeze the final context and store it behind a handle; specify handle
   lifetime, invalidation, sharing, and failure behavior.
4. Support the selected dialect, including SQLite's supported schema loading
   pathway, without assuming PostgreSQL-only inputs.
5. Define ownership and precedence of metadata settings versus request
   overrides; remove any `unwrap()` on an unrecognized datasource.
6. Specify partial-initialization behavior and context identity semantics.

## Query analysis

### Existing server-side CTE realization is authoritative

`ResolvedCteCatalog` is a shared `sql-fun-core` data type, not a transport
format to reinvent. The server already owns
`realize_cte_catalog(&ResolvedCteCatalog, Dialect)`. It produces:

- a common AST with catalog CTE definitions prepended to existing WITH entries;
- `OrderedBindings`, with the CTE placeholder-token order first, followed by
  the full base query's placeholder-token order.

The order is the numbered-placeholder contract for downstream analysis and
`LockfileEntry.bind_parameters`. Do not rederive or alphabetically sort it
in the CLI or the response.

```text
CLI: cargo expand -> QueryRequirements::extract
    | selected datasource + ResolvedCteCatalog + source identity
    v
Server: ContextHandle lookup
    -> realize_cte_catalog (common AST + OrderedBindings)
    -> semantic analysis against frozen schema
    -> diagnostics / binding descriptions / result-set shape / accesses
    v
CLI: associate with Rust location; validate or update lockfile
```

### Proposed request

For the `sql-fun check` path, the logical minimum is:

```text
analyze_query:
    context_handle
    resolved_cte_catalog: ResolvedCteCatalog
    query_identity (optional correlation identifier)
```

The resolved catalog already contains the base query. Passing both
`AnalyzeQueryInput::Text` and a catalog containing the same query introduces
two potentially conflicting sources of truth; avoid that duplication.

The existing `Text/File/Url` mode may still be useful for general SQL analysis.
If retained, model raw SQL and resolved-catalog analysis as **distinct input
variants**, not independently optional conflicting fields. Whether URL inputs
are needed, and whether a server should fetch remote content, remain undecided;
they are not required by `check`.

A batch request is not required for the initial contract: multiple requests
can share an initialized context. Request correlation must be unambiguous even
when responses arrive asynchronously.

### Proposed response

A successful analysis result should provide, per statement:

| Output | Required meaning |
| --- | --- |
| Statement identity and kind | Identify analyzed input/statement, including a failure that prevents kind determination |
| Executable SQL | The realized SQL representation required for lockfile/codegen, if available |
| Bindings | Stable, ordered mapping of placeholder position to source parameter identity, inferred type constraints, and nullability |
| Result columns | Ordered names, SQL types or type candidates, and nullability |
| Relation and column accesses | Resolved object references where available, for inventory and diagnostics |
| Diagnostics | Syntax/semantic errors and warnings, including unresolved relations/columns and ambiguous references |
| Analysis completeness | Distinguish fully analyzed, partially analyzed, and no analyzable result without fabricating types or objects |

These are **required capabilities**, not a claim that the current
`AnalyzeQueryItem` fields already provide them. In particular, its
`query_kind` is mandatory, its `ParameterDescribe` contains only name,
spans, candidates, and nullability, and its `ResultSetColumn` only has a
single optional type. Review those contracts against the real semantic nodes
before choosing final serializable shapes. SQLite type alternatives and
unresolved/contradictory inference must remain representable; do not reject
legal dialect constructs because they are uncommon.

A response must identify the exact binding order supplied by CTE realization.
The server may produce SQL-specific binding constraints; checking Rust function
signatures and displaying diagnostics at Rust source positions remains a CLI
concern unless a separate explicit API contract is agreed.

### Diagnostics and location mapping

Current `AnalysisProbrem` includes a code, file path, statement span, and
problem span. The query API additionally needs at least a human-readable
message, severity, source identity, and a defined coordinate system. Related
spans are useful when ambiguity or conflicting constraints involve multiple
locations.

Diagnostics must not assume all spans refer to one post-expansion SQL string.
A catalog CTE is sourced separately from the base query, and AST mutation
may move or introduce text. Specify whether spans point to original base SQL,
the referenced CTE definition, or generated SQL. If a precise mapping is
unavailable, report the correct source with an unavailable/whole-statement
location rather than invent a precise offset.

Do not treat unknown table/column errors as transport failures. Return useful
semantic diagnostics and retain independently valid information when sqlast
supports recovery. Distinguish recoverable `AnalysisProblem` from fatal
`AnalysisError`; whether fatal parse/analysis errors can always be converted
to structured diagnostics requires investigation. JSON-RPC errors should
represent invocation, state, and service failures, not ordinary invalid SQL.

### Collections and state

Existing `Collection<T>`, `read_collection`, and `release_collection` are
planned mechanisms to page or release large results. Their implementation is
outstanding. Collections should belong to the authenticated connection's
session and must not become accessible from another client's session.
Define handle expiry, resource bounds, release behavior on disconnect, and
what happens when the cap is reached. Schema-context cache sharing and
connection-scoped collection ownership are separate concerns.

## Work required before the API can be declared implemented

1. Verify sqlast query analysis APIs for bind type inference, result column
   nullability, relation/column provenance, and structured recoverable errors.
2. Confirm an end-to-end conversion from realized common AST into the
   dialect's semantic analysis path, including executable SQL deparsing.
3. Establish origin mapping for diagnostics across CTE realization.
4. Define stable serialization of type alternatives and incomplete inference.
5. Define request variants, response completeness, context/collection lifetime,
   and the exact division between RPC errors and analysis diagnostics.
6. Implement server dispatch for `analyze_query`, schema initialization,
   context cache, and collection operations.
7. Connect `sql-fun check` to those operations and test successful analysis,
   missing table/column, parameter ordering, type ambiguity, multi-datasource
   isolation, and both supported dialects.

This document deliberately leaves unresolved contract decisions visible.
It does not authorize behavior changes, schema/API type changes, or implementation
work beyond documenting the design.
