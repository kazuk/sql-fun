# SQLite project bootstrap

> **Planned / not implemented yet:** This page describes the target SQLite
> bootstrap workflow. The `sql-fun-sqlite` helper, extraction API, project-local
> snapshot workflow, dedicated diagnostics, and official binary policy below
> are planned. Their names and interfaces may change. The planned examples are
> illustrative and cannot yet be used as working setup instructions.

This page is for application developers preparing a SQLite project for sql-fun.
The goal is to analyze queries using engine information that matches the SQLite
runtime their application actually uses.

## What works today

SQLite support is under development and requires the `sqlite` feature on the
proc-macro, CLI, and server. Follow the current
[installation instructions](installation.md) for those features and tools.
See [SQLite parser limitations](sql-analysis.md#sqlite-parser-limitations) and
[SQLite analysis coverage](sql-analysis.md#sqlite-analysis-coverage) for the
current limits. Enabling the feature does not provide the planned extraction
workflow described here.

[`sql-fun check`](cli/check.md) also requires `cargo-expand` on `PATH`.
The [installation guide](installation.md) explains how to install and verify it.

## Information used by the workflow

- **project metadata** is sql-fun configuration in Cargo
  `[package.metadata.database...]`, together with equivalent environment-variable
  overrides where supported. The term remains project metadata regardless of
  how the configuration is supplied.
- **engine builtin** is engine-provided information needed for semantic
  analysis: schemas and tables/views where applicable, types and affinities,
  functions, operators, casts, capabilities, compile/runtime options,
  extensions, and other relevant engine facts.
- **user schema** is the application's own schema, including its tables and
  views. The application remains responsible for its schema and migrations.
- **query inventory** is sql-fun's query-level analysis/inventory data. It is
  distinct from the engine builtin snapshot and user schema used to analyze
  those queries.

SQLite has no PostgreSQL-style external server catalog that can serve as the
single source of engine builtin information. The relevant runtime belongs to
the application: its behavior depends on the connector, linked SQLite library,
compile options, bundled or system library configuration, and loaded extensions.
A SQLite connection opened by sql-fun itself could use a different library or
configuration and expose different capabilities. Inspecting that connection
would therefore not reliably describe the application's runtime.

## 1. Configure project metadata

The following uses the existing project metadata structure and selects the
`rusqlite` connector in `Cargo.toml`:

```toml
[package.metadata.database.default]
engine = "sqlite"
dialect = "SQLite"
connector = "rusqlite"
```

The `dialect` field explicitly selects SQLite SQL parsing and is required by
current metadata loading. The datasource is named `default`, which is also the
name used when a query macro omits its `datasource` attribute. A named datasource
uses a corresponding table, such as `[package.metadata.database.edge]`, and
queries select it with `datasource = "edge"`.

Existing supported environment-variable overrides remain project metadata.
For example, `SQL_FUN_SCHEMA` selects the schema/output variant described in
[Installation](installation.md#setting-environment-variables). The configuration
above selects the SQLite dialect through Cargo project metadata.

## 2. Add the extraction helper

**Planned / not implemented yet:** Add the intended helper as a development
dependency with a connector-specific feature:

```toml
[dev-dependencies]
sql-fun-sqlite = { version = "...", features = ["rusqlite"] }
```

The version is a placeholder, and the crate name and feature/API naming remain
provisional. This is not currently an installable dependency example.

The selected connector matters: sql-fun code generation supports specific
connectors, so the helper should provide explicit adapters for those connectors.
The `rusqlite` feature is intended to accept a `rusqlite` connection; it does not
imply that every SQLite connection type shares a universal interface.

## 3. Extract engine builtin from the application's runtime

**Planned / not implemented yet:** Add an application-side development/test
helper that obtains the same kind of connection the application normally uses
and passes it to the extraction helper. For example, the target API could look
like this:

```rust,ignore
#[test]
fn extract_sql_fun_engine_builtin() {
    let conn = open_application_database();

    sql_fun_sqlite::rusqlite::extract_engine_builtin(
        &conn,
        ".sql-fun/sqlite",
    )
    .unwrap();
}
```

This entire example is illustrative. `open_application_database()` represents
application-owned connection setup, and the extraction function is a planned
API rather than a function available today. Use the application's normal
connector, SQLite library/build options, and extension loading when preparing
that connection. Run the extraction helper during development/test once it is
implemented.

The intended output is an observed engine builtin snapshot in project-local
sql-fun state, conceptually under:

```text
.sql-fun/sqlite/
```

The directory is a target location, not a promise about current automatic
loading. Snapshot filenames, format, and layout for multiple runtimes are not
fixed by this workflow. Extracting engine builtin information does not replace
maintaining the application's user schema.

## 4. Use the snapshot during analysis

**Planned / not implemented yet:** Subsequent sql-fun operation should combine
these inputs:

```text
project metadata
+ engine builtin
+ user schema
-> semantic analysis / sql-fun check / server
```

Once a suitable project-local snapshot exists, the normal analysis path should
use it without reconnecting to the SQLite runtime merely to rediscover engine
builtin information. The query inventory represents query-level analysis data,
separate from that runtime snapshot.

This is the intended consumption model; it does not mean that current SQLite
analysis already loads snapshots from `.sql-fun/sqlite/` or validates every
SQLite query.

## 5. Refresh when the runtime changes

**Planned / not implemented yet:** Rerun extraction when relevant runtime
characteristics change, including:

- the connector or runtime implementation;
- the SQLite version;
- build or compile options;
- bundled versus system SQLite configuration;
- enabled or loaded extensions.

Use a connection with the new configuration so the refreshed engine builtin
snapshot describes the runtime that will execute your queries. Automatic
staleness detection is not established by this plan; refreshing after these
changes is part of the intended development workflow.

## 6. Understand support and failure boundaries

Current local builds need the `sqlite` feature to include SQLite support, as
shown in [Installation](installation.md).

**Planned / not implemented yet:** The bootstrap workflow should distinguish
these failure boundaries:

| Condition | Intended action |
| --- | --- |
| The binary was built without SQLite/dialect support. | Install or build a binary with SQLite support. |
| The binary supports SQLite, but the required project-local engine builtin snapshot is absent. | Run extraction using the application's SQLite connection. |
| The binary supports SQLite, but the snapshot no longer represents the application's runtime. | Refresh the snapshot using the changed runtime. |

Compiling in SQLite support and obtaining project-local engine builtin data are
separate setup requirements. Dedicated snapshot diagnostics and exact error
strings are not specified or claimed to exist today.

## Official binaries and local builds

**Planned / not implemented yet:** Official sql-fun CLI/server binaries should
include all supported engine features so they can answer both PostgreSQL and
SQLite semantic-analysis requests. Users building sql-fun themselves should
remain free to disable SQLite or other optional features.

Project-local engine builtin data should depend on the project's selected
engines, independently of which engine implementations are compiled into its
binary. Under this target model, a SQLite-only project does not need PostgreSQL
engine builtin data just because the official server binary includes PostgreSQL
support. This is the intended distribution and analysis behavior, not a claim
that current packages already implement it.
