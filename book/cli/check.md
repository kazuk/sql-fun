# `check` sub command for `sql-fun` CLI utility

Check the application embedded queries and prepare to build.

`check` sub command is core of application development workflow.

- Scan a schema dump and enumerate schema objects.
- Scan a application source code and enumerate queries.
- Validate all queries syntax and semantics.
- Build CRUD table
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

For each referenced datasource, a completed check regenerates the entire
`<datasource>.<schema>.lock` file, ordered by Rust query function name. Only
eligible functions analyzed in that invocation are included. Old entries absent
from the selected targets disappear. A function whose analysis lacks executable
SQL or required parameter/result metadata is omitted, including its old entry.
SQL analysis failures make the command fail, but eligible entries are still
saved. Connection, extraction, initialization, and RPC failures prevent saving
an incomplete inventory.

If no SQL query functions are extracted, `check` warns and succeeds without
opening an analysis session or reading or changing any lockfile. This also applies
to `--validate-only`, even if a datasource was selected. It does not clean up old
lockfiles when all queries have been removed.

A single-statement entry keeps its SQL, bindings, and result shape directly on
`[[entries]]`. Multiple statements retain their order and independent metadata
in `[[entries.statements]]` tables. Acceptance by analysis does not guarantee that
the selected connector can execute multiple statements; proc-macro/codegen makes
that capability decision and reports unsupported operations.

Diagnostic rendering is separate work tracked by #699. Analysis results retain
their diagnostics even when no lockfile entry can be produced.

## Prerequisites

`sql-fun check` requires `cargo-expand` for both PostgreSQL and SQLite.
See the [installation guide](../installation.md) for setup instructions.
The server must be installed on `PATH` or selected through `SQL_FUN_SERVER`;
`USER` selects its namespace, and `SQL_FUN_SERVER_COMMAND` can supply the launch
command as described for [`api`](api.md). Requirements select their datasources
from project metadata. Schema input follows the common `--schema-file` / `--schema`
selection and must be visible to the server.

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

Default : Cargo default package to check.

- `--workspace`: Check all package in the cargo workspace
- `--package <NAME>`, `-p <NAME>`: package to check

### Target selection options

- `--lib`: check only this package's library
- `--bin <NAME>`: check only the specified binary
- `--example <NAME>`: check only the specified example
- `--test <NAME>`: check only the specified test
- `--tests` : include tests when check the lib or bin
- `--bench <NAME>`: check only the specified bench

### Feature selection options

- `--features FEATURE-LIST`: space or comma separated list of features
- `--all-features`: Activate all available features
- `--no-default-features`: Do not activate the `default` features

### Lockfile control options

- `--force-rebuild`: Start with an empty initial lockfile state. All queries are
  analyzed and the entire lockfile is regenerated, as in normal mode. This treats
  every entry as new; it does not invalidate the server schema cache. Conflicts
  with `--validate-only` and `--expect-schema-dump-valid`.
- `--validate-only`: Compare prospective lockfiles without creating, writing, or
  deleting files. Changes to entries, their name ordering, or required headers
  fail validation; TOML whitespace alone does not. When prospective entries are
  empty, an absent file and an existing valid empty file both succeed. Existing
  files still require valid matching version/schema-hash headers, even when
  empty. Stale nonempty files, malformed TOML, and read errors fail validation.
- `--expect-schema-dump-valid`: Fail if schema dump changed

## Progress output

Use `--progress silent`, `--progress dots`, or `--progress names`, or set
`SQL_FUN_CHECK_PROGRESS` to the corresponding value. The command-line option
takes precedence over the environment; the default is `names`.

- `silent`: no progress output; warnings and diagnostics remain enabled.
- `dots`: one dot for each fully received query-function analysis, including
  unsuccessful SQL analysis. A function containing several statements counts
  once. The line ends cleanly on completion or failure.
- `names`: report each analyzed function name and the completed/total counts.

Progress includes extraction, analysis, and lockfile finalization phases. It goes
only to **stdout**, including when redirected. Select `silent` if stdout must be
free of progress. Warnings and diagnostic rendering use **stderr**. A completed
analysis count does not imply successful finalization: errors report termination
instead of successful completion.
