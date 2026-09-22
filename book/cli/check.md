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

## Prerequisites

Install `cargo-expand` before running `sql-fun check`; the command invokes
`cargo expand` to read generated query stubs. This is a required runtime tool
for both PostgreSQL and SQLite. See the [installation guide](../installation.md)
for installation commands and the `PATH` check.

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
