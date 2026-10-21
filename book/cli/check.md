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

`check` extracts requirements from expanded Rust source and submits their resolved
CTE catalogs to `sql-fun-server`. It reuses schema contexts for requirements from
the same datasource on one connection. The catalog's base query is the sole SQL
input; CTE files are not loaded again by the server.

Diagnostic presentation, lockfile updates and successful command completion are
still unfinished (#689). After receiving analysis responses, `check` currently
reaches an explicit unimplemented marker; it is not yet a completed validation
command. The schema-cache policy options below remain pending integration.
Server schema-context initialization currently supports PostgreSQL.

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

### Schema cache control options

- `--force-rebuild`: Clean schema cache before checking
- `--validate-only`: Fail if schema dump or SQL statement changed
- `--expect-schema-dump-valid`: Fail if schema dump changed
