# `sql-fun check`

Check the application embedded queries and prepare to build.

`check` sub command is core of application development workflow.

- Scan a schema dump and enumerate schema objects.
- Scan a application source code and enumerate queries.
- Validate the syntax and semantics of extracted queries.
- Generate a CRUD/query inventory.
- Prepare query information for `sql-fun` procidual macro
  - `sql-fun` procidual macro uses schema cache default
    - When schema cache not found or invalid:
      - uses `sql-fun-server` and schema aware validation in on-demand.

For supported SQL forms and known analysis limitations, see
[SQL analysis coverage and limitations](../sql-analysis.md).

## Implementation status

`check` extracts queries from the selected Cargo targets and submits each query
with its resolved CTE dependencies to `sql-fun-server`. It reuses schema contexts
for queries from the same datasource. Server initialization currently supports
PostgreSQL; SQLite-enabled builds do not imply SQLite initialization support.

Before extraction, `check` forwards the common datasource configuration to
`cargo expand`. The global `--datasource` option takes precedence over
`SQL_FUN_DATASOURCE`; otherwise `default` is used. If the selected datasource
has no metadata entry, its metadata-derived settings are omitted from the
transferred configuration. Each datasource referenced by a query must have
valid metadata, including `default` when a query uses it.

For each referenced datasource, a completed check regenerates the entire
`<datasource>.<schema>.lock` file, ordered by Rust query function name. Only
eligible functions analyzed in that invocation are included. Old entries absent
from the selected targets disappear. A function whose analysis lacks executable
SQL or required parameter/result metadata is omitted, including its old entry.
SQL analysis failures make the command fail, but eligible entries are still
saved. When `check` reports a connection, extraction, initialization, or RPC
failure, it does not save partial lockfiles or CRUD/query inventories.

If no SQL query functions are extracted, `check` warns and succeeds without
opening an analysis session or reading or changing lockfiles or CRUD inventories.
This also applies to `--validate-only`, even if a datasource was selected. It does
not clean up old artifacts when all queries have been removed.

Macro expansion can emit a compilation error without producing a query that
`check` can extract, for example when the query's datasource is not configured.
That query is not analyzed. `check` can report no SQL analysis errors and may
exit successfully, including with the no-queries warning, despite the compilation
error. A successful `check` covers only extracted queries and does not establish
that the Rust application compiles. Run `cargo check` or `cargo build` as well;
they fail if the underlying macro compilation error remains.

A single-statement entry keeps its SQL, bindings, and result shape directly on
`[[entries]]`. Multiple statements retain their order and independent metadata
in `[[entries.statements]]` tables. Acceptance by analysis does not guarantee that
the selected connector can execute multiple statements; proc-macro/codegen makes
that capability decision and reports unsupported operations.

## CRUD/query inventory

For each datasource referenced by the selected targets, `check` also writes
`sql-fun.<datasource>.<schema>.toml` beside the lockfile, in the directory containing
the selected metadata file. Here `<schema>` is the selected schema-dump variant,
not a SQL schema name. For example, datasource `default` and variant `develop`
produce `sql-fun.default.develop.toml`.

The inventory identifies which queries access each SQL-visible relation and
column, preserving detailed access purposes such as `UpdateTarget`, `UpdateInput`,
`UpdateFilter` and `UpdateReturning`. A column can appear under several purposes
in one statement. Relation-level `DeleteRows` does not imply access to every
column. Purposes describe the enclosing top-level operation: column references
inside a scalar subquery in SELECT output retain `SelectOutput`, including
references written in that subquery's WHERE.

See the [SQL access-purpose reference](../access-kinds.md) for every purpose,
SQL examples, and the attribution rules for nested queries and CTEs.

The TOML contains:

- `inventory_version`, `datasource`, `schema_variant`, `schema_dump_hash`: artifact
  format and selected input identity.
- `[[queries]]`: Rust `function`, `source_file`, `source_line`, query/CTE
  `fingerprint`, and statement count `statements`. This also lists queries with
  no object accesses. For proc-macro declarations, source fields identify the
  Rust macro invocation using the compiler-reported file path and line. Paths
  are normally relative to the defining crate and respect compiler path remapping.
  Other extracted inputs can retain `<unknown>` when a file name is unavailable.
- `[[relations]]`: SQL object `kind`, optional `catalog`/`schema`, and `name`.
  Names are logical identifier components; embedded dots are not separators.
- Each relation's `accesses` is a mapping from an exact AccessKind name to an
  array of responsible query references. For example,
  `[[relations.accesses.DeleteRows]]` identifies a query deleting rows.
- `[[relations.columns]]` gives the column `name`. Its `accesses` uses the same
  access-kind → query-list mapping, such as
  `[[relations.columns.accesses.UpdateInput]]` for assignment-expression reads.
- Every reference directly contains `function`, `source_file`, `source_line`,
  `fingerprint` and a one-based `statement` index within that query. There are no
  numeric query IDs to look up. Different source locations remain distinct;
  fingerprints also distinguish different SQL/CTE inputs when extracted source
  locations collide. All analyzed queries are retained rather than overwritten
  when their extracted source identities coincide. Identical access references
  within one kind are deduplicated; different roles remain separate.

Empty access mappings are valid. The nested structure can be enumerated directly
as relation → access purpose → queries, or relation → column → access purpose →
queries. Converting TOML to JSON and applying an ordinary presentation template
is sufficient to render a CRUD matrix or impact report: no SQL interpretation,
ID joins or data reshaping is required. See the [tested template and rendered
matrix](../introduction.md#rendering-a-crud-matrix).

The inventory does not duplicate executable SQL or Rust type metadata; lockfiles
continue to serve code generation.

Aliases, CTEs and subqueries use the resolved accesses returned by the server.
A view reference identifies that view and its columns; a table-function output
identifies the function's output columns. View definitions and function bodies
are not recursively inspected. This is a query/schema usage inventory, not proof
of access-control correctness or a mapping to business functions.

Each touched datasource's inventory covers **only the queries selected by this
invocation**. Regeneration removes entries absent from that selection; other
datasources' files are untouched. Repeating the same check with unchanged inputs
produces identical bytes. No timestamp or server-session identity is stored.

Inventory collection is independent of Rust generation eligibility. SQL
diagnostics are still reported and the command retains its existing failure
result when a lockfile entry cannot be produced; returned access facts remain in
the inventory. When `check` reports an extraction, initialization, RPC, or
connection failure, it does not publish inventories, leaving previous files
stale. Each inventory is replaced atomically after analysis/connection
completion. Lockfiles and inventory
files are not replaced as one transaction; inspect the command result before
using generated artifacts.

See the [fixture-backed introductory example](../introduction.md#crud-table-generation).

## SQL input rules

A query function may contain multiple SQL statements when no catalog CTE
expansion is required, including when catalog resolution produces zero referenced
CTEs. Each statement keeps its own parameters and result columns in source order.

A query function that uses catalog-resolved CTE expansion must contain **exactly
one base SQL statement**. Combining multiple base statements with referenced
catalog CTEs is an invalid query definition. Analysis produces a diagnostic,
`check` fails as it does for SQL syntax or analysis errors, and the function is
omitted from the generated lockfile, including any previous entry for it.
This is an SQL input rule, independent of the connector's execution capabilities.

## Prerequisites

`sql-fun check` requires `cargo-expand` for both PostgreSQL and SQLite.
See the [installation guide](../installation.md) for setup instructions.
The server must be installed on `PATH` or selected through `SQL_FUN_SERVER`;
`USER` selects its namespace, and `SQL_FUN_SERVER_COMMAND` can supply the launch
command as described for [`api`](../api/cli-api.md). Requirements select their datasources
from project metadata. Schema input follows the common `--schema-file` / `--schema`
selection and must be visible to the server.

`SQL_FUN_MAX_RESPONSE_BYTES` sets the authenticated response receive cap
(default 4 MiB). See [response size and memory use](../api/cli-api.md#response-size-and-memory-use)
for how to raise it after a size-limit error.

## Syntax

```text
sql-fun [COMMON OPTIONS] check [OPTIONS]
```

There are no positional arguments. Place [common options](index.md#common-options)
before `check`. The command returns 0 on success, 1 on validation or operational
failure, and 2 for invalid CLI usage. A successful check does not guarantee that
the application compiles, as described above. `-h` / `--help` prints help.

## Usage

### Application development stage

```console
sql-fun check
```

### CI workflow

Fail for application queries not up to date with schema dump.

```console
sql-fun check --validate-only
```

## Options

### Package selection options

By default, checks workspace default packages that depend on `sql-fun`.
`--workspace` and `--package` conflict.

- `--workspace`: Check workspace packages that depend on `sql-fun`.
- `--package <NAME>`, `-p <NAME>`: package to check

### Target selection options

- `--lib`: check only this package's library
- `--bin <NAME>`: check only the specified binary
- `--example <NAME>`: check only the specified example
- `--test <NAME>`: check only the specified test
- `--tests` : include tests when check the lib or bin
- `--bench <NAME>`: check only the specified bench

### Feature selection options

- `-F`, `--features FEATURE-LIST`: space or comma separated list of features
- `--all-features`: Activate all available features
- `--no-default-features`: Do not activate the `default` features

`--all-features` conflicts with `--features` and `--no-default-features`.

### Lockfile control options

- `--force-rebuild`: Start with an empty initial lockfile state and regenerate
  inventory without loading its previous content. All queries are
  analyzed and the entire lockfile is regenerated, as in normal mode. This treats
  every entry as new; it does not invalidate the server schema cache. Conflicts
  with `--validate-only` and `--expect-schema-dump-valid`.
- `--validate-only`: Compare prospective lockfiles without creating, writing, or
  deleting files. Changes to entries, their name ordering, or required headers
  fail validation; TOML whitespace alone does not. When prospective entries are
  empty, an absent file and an existing valid empty file both succeed. Existing
  files still require valid matching version/schema-hash headers, even when
  empty. Stale nonempty files, malformed TOML, and read errors fail validation.
  Inventory validation also compares the prospective parsed document, including
  query/source identity, membership, detailed accesses and headers. Missing expected
  inventories, stale content, malformed TOML and unsupported versions fail;
  whitespace alone does not. Queries with no object accesses still require an
  inventory. The zero-extracted-query behavior described above takes precedence.
- `--expect-schema-dump-valid`: Fail if an existing lockfile or inventory records
  a different schema-dump hash.

### Compilation and manifest options

These options are passed to `cargo expand` for selected packages:

- `-j`, `--jobs <N>`: parallel jobs; Cargo chooses the default.
- `--release`: use the release profile.
- `--profile <NAME>`: use the named profile.
- `--target <TRIPLE>`: select a compilation target.
- `--target-dir <PATH>`: select the build-artifact directory.
- `--manifest-path <PATH>`: supply a manifest to `cargo expand`. Package
  selection still uses Cargo metadata from the current workspace.

The `--frozen`, `--locked`, and `--offline` options are passed to `cargo expand`
and to `cargo metadata` when selecting default or workspace packages. Cargo and
cargo-expand interpret these options according to their own behavior. Apart from
`SQL_FUN_CHECK_PROGRESS`, the options specific to `check` have no environment
variable equivalents.

## Progress output

Use `--progress silent`, `--progress dots`, or `--progress names`, or set
`SQL_FUN_CHECK_PROGRESS` to the corresponding value. The command-line option
takes precedence over the environment; the default is `names`.

- `silent`: no progress output; warnings and diagnostics remain enabled.
- `dots`: one dot for each fully received query-function analysis, including
  unsuccessful SQL analysis. A function containing several statements counts
  once. The line ends cleanly on completion or failure.
- `names`: report each analyzed function name and the completed/total counts.

Progress includes extraction, analysis, and artifact finalization phases. It goes
only to **stdout**, including when redirected. Select `silent` if stdout must be
free of progress. Warnings and diagnostic rendering use **stderr**. A completed
analysis count does not imply successful finalization: errors report termination
instead of successful completion.

## SQL diagnostics

Look up reported codes in the [diagnostic reference](../diagnostics/)
for causes, limitations, examples and suggested corrections.

`check` reports every SQL analysis diagnostic on stderr, including errors,
warnings, and advice for queries that cannot produce a lockfile entry. Each
report identifies the Rust query function and its source file and line, and
shows the complete original SQL query. When the server retains a reliable
statement position, the report also highlights the relevant SQL and labels its
source span. Diagnostics without reliable positions still show their code,
explanation, and complete query without a guessed location. `--progress silent`
suppresses progress output, not diagnostics.
