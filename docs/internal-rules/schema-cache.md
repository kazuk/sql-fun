# Server schema cache

The server's `Api::schema_cache_service` owns one `SchemaCacheService` for the
process lifetime. Its first explicitly supplied builtin installation fixes
that service's scope. Different installation paths are rejected; restarting
allows selecting another installation. Extracted builtin JSON masters
are fixed for the service lifetime; manually modifying them is unsupported.
Restart following re-extraction. Builtin `schema.sql` is the first ordinary
watched SQL file layer after the fileless frozen root, before extensions and
the application dump. Its edits are observed without restarting.

The initial service supports PostgreSQL. The SQLite file reader exposes the
same original-content hash contract, without adding SQLite initialization to
the historical PostgreSQL-only RPC. Session handle registration and collection
responses remain in #428; initialization RPC integration is pending that work.
The cache service and its independently owned result API do not require it.
The CLI's client hash calculation remains downstream work.

## Identities and ownership

`SchemaContextKey` identifies content, not access authority. The root hashes,
in order, dialect display, engine-version display, and JSON of the effective
ordered search path using `sql_fun_core::Hasher::append_str`. File layers hash
parent key, original file-content hash, and canonical UTF-8 absolute path in
that order. Paths that cannot be represented without loss are rejected.

Search paths are resolved through `SqlFunArgs::postgres_search_path`, including
the selected PostgreSQL engine default (`["public"]`) when unspecified. The
resolved ordered value enters the root identity regardless of its source.

The service has a single immutable installation scope, so root identities do
not need an installation or datasource suffix. Extension file identities use
their canonical paths and original contents. Their supplied order matters.

Roots use deliberately constructed schema-only metadata and arguments, not
configuration retained from whichever datasource first loaded a prefix.
`SelectedContext` in the consuming server crate overlays each selected
request's metadata/arguments while delegating every catalog read. The sqlast
model is unchanged. Callers obtain that view from `InitializedSchema` and
create a separate mutable context for each query. Raw shared analyses expose
schema-only configuration, never another datasource's configuration.

Successful analyses retain their frozen catalogs and ordered, file-associated
recoverable diagnostics. Prefix diagnostic objects use shared ownership. Syntax
and fatal semantic failures are classified using `ParseDumpFileError::is_io`
and retained as immutable messages: `AnalysisError` can contain non-Send error
payloads, so those payloads cannot cross the blocking-task boundary. I/O errors
are reported to waiters but never stored as schema outcomes.

No schema eviction or memory budget is implemented. Independently owned results
continue to work after the service is dropped and can support future eviction.

## Loading and invalidation

A short cache mutex atomically registers each oneshot receiver and spawns the
server-owned loader. Dropping all receivers does not cancel the task. No cache
lock covers filesystem reads, parsing, semantic analysis, or query usage.
Synchronous work runs in Tokio blocking tasks. Completed deterministic results
are published under the actual read-content identity before request validation.

The file-hash cache watches only explicitly selected canonical files. It checks
device/inode identities, re-registers replacement watches, and advances a
per-path generation on invalidation. A read can repopulate a path hash only
when its generation and file identity still match. Retaining generation records
prevents an old read from recreating an invalidated hash entry. Watch callbacks
never wait on watch operations while holding their shared state mutex.

Watcher failures or rescan signals invalidate hashes. Failed watch registration
prevents hash retention, allowing subsequent explicit reads without pretending
the optimization is reliable. Native watch events plus inode checks are a
pragmatic local-filesystem optimization, not perfect coherency. Invalidation
does not delete or rebuild immutable schema analyses.

Path loads coalesce by parent identity, canonical path, invalidation
generation, and observed content hash. Different observations cannot join an
older in-flight load even when file watching is unavailable. The file-hash cache
never reads or hashes SQL: it retains only hashes produced by sqlast snapshots.
A valid retained hash can select an existing analysis without reading SQL again.
Otherwise sqlast reads the file once into an immutable `SqlFileSnapshot`, hashes
that exact original text, and uses the snapshot identity for cache lookup and
in-flight sharing. On a miss, analysis consumes the same snapshot without
reopening the path. PostgreSQL preprocessing operates on the captured text and
preserves original byte offsets and line endings. SQLite uses the same snapshot
reader without PostgreSQL preprocessing.

Edits or deletion after snapshot capture cannot change the hash/analysis pairing.
Successful analyses and deterministic failures use the captured content identity.
A late path hash population cannot restore data invalidated while reading.
Temporary snapshots are dropped after lookup or analysis; their text is not
retained in the schema cache.

`expected_hash` first selects a retained content-addressed application layer for
that parent schema and canonical dump identity. A hit supplies the historical
success or deterministic failure without reading the current dump. Changing or
deleting the dump does not prevent reuse. Previously resolved absolute input
paths retain their canonical identities for this lookup; watcher invalidation
clears path-hash hints, never these identities or content-addressed analyses.
New aliases must first be resolved to their canonical identity.

On a miss, read and analyze the current snapshot. Publish its actual-content
outcome before comparing the request hash. If the client expects A and the
snapshot contains X, retain X and return a mismatch; retry expecting X reuses
that outcome. Mismatch errors are never negative schema-cache entries. A changed
parent (builtin SQL, extension order/contents, or root configuration) is a
different hierarchical identity and does not select an old parent's analysis.
Historical entries remain in memory only for the service lifetime.

## Panic policy and validation

The server executable installs a default-diagnostic-preserving panic hook
before its Tokio runtime is created. Any panic aborts that server process,
including async/blocking task panics. Importing the library does not install
the hook. No panic recovery or cache repair is attempted. Subprocess tests
verify abort and backtrace output without killing the main runner.

Unit tests run in the maintained workspace/nextest flow, with small synthetic
catalog fixtures. Full initialization RPC tests must be added when #428 wires
session registration to this service. `notify` 8.2 supports Rust 1.77 and its
Linux backend uses native inotify through existing Rust/libc dependencies;
there is no additional executable or native development package prerequisite
for the repository's Linux CI image.
