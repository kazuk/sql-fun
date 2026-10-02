# Connection Manager Module Generation

*This is a planning document outlining a potential future feature of
`sql-fun`.
It is not yet implemented, and no timeline or commitment has been established.*

## Motivation

`sql-fun` currently leaves connection ownership, pooling, and transaction
boundaries to the application.

That responsibility should remain with the application. However, much of the
connector-specific setup around those responsibilities is repetitive and can be
generated from datasource metadata and application-selected policy.

A module-level procedural macro could provide a single namespace for that
generated integration:

```rust
#[connection_manager(
    datasource = "main",
    connector = "tokio-postgres",
    pool = "deadpool"
)]
mod main_connection;
```

The generated module could contain connector-specific types and helper
functions for acquiring execution resources without making `sql-fun` the owner
of their lifecycle.

## Application-Owned Lifecycle

The generated API must preserve explicit application control over connection
and transaction boundaries.

For example, an application may still write code conceptually like:

```rust
let mut tx = main_connection::begin(&pool).await?;

main_db::orders::insert(&mut tx, ...).await?;
main_db::ledger::insert(&mut tx, ...).await?;

tx.commit().await?;
```

The generated module may remove connector boilerplate, but it must not
implicitly decide where transactions begin, commit, or roll back.

This keeps connection and transaction policy visible in application code and is
consistent with the existing `sql-fun` model.

## Datasource Execution Context

The name `connection_manager` describes the common database case, but the
underlying abstraction may be broader than a connection pool.

Different connectors may obtain an execution context in different ways:

- a PostgreSQL connector may acquire a pooled connection
- SQLite may use an application-owned database handle
- WASI SQL may use an ambient capability or host-provided resource
- a transaction-monitor connector may obtain an implicit transaction context
  from the surrounding execution environment

Conceptually, the generated module is a datasource-specific execution-context
adapter.

This allows connector policy to vary while query code continues to receive the
resource required to execute against the datasource.

## Relationship to Schema Generation

Connection management and schema projection should remain separate concerns even
when both refer to the same datasource:

```rust
#[schema_objects(datasource = "main")]
mod main_db;

#[connection_manager(datasource = "main")]
mod main_connection;
```

The first module projects the datasource's static database objects into Rust.
The second projects the application's configured execution path for that
datasource.

Keeping them separate avoids coupling schema semantics to a specific pool,
connector, TLS implementation, or transaction-management environment.

## Zero-Runtime Constraint

The feature should generate ordinary application code and connector integration.
It should not require a new `sql-fun` runtime framework or global connection
registry.

Whether a selected connector or pool crate has its own runtime dependencies is a
property of that generated integration, not a new runtime dependency imposed by
the `sql-fun` query/codegen model itself.

## Rust Version Requirement

Rust 1.99 stabilizes custom attribute and derive macros around outlined modules
such as:

```rust
#[some_attribute]
mod generated;
```

That syntax provides a natural declaration point for generating a coherent
connector module.

The current `sql-fun` MSRV is lower than Rust 1.99. Therefore this feature is
planning-only while that remains true.

Implementation should only be considered after the project MSRV has independently
moved to Rust 1.99 or newer. This feature should not itself be used as the reason
to raise the MSRV.

## Open Questions

Areas to decide during implementation planning include:

- the connector configuration model
- whether pool integrations are built in, externally supplied, or template-based
- generated ownership and borrowing APIs for connections and transactions
- how async and synchronous connectors coexist
- TLS and authentication configuration boundaries
- error-type exposure
- support for execution contexts that are not conventional connection pools
- interaction with generated schema/query modules
