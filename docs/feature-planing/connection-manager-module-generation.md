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

A possible future module-level procedural macro API is:

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

## Example-First Exploration

Before committing to a connection-management proc macro, provide this idea as an
example-side generator driven by project metadata and a Tera template.

Conceptually:

```text
project/datasource metadata
            |
            v
        Tera template
            |
            v
 generated connector module
```

The template should live under `examples/` and be linked from this planning
document. It is an example, not a supported connection framework and not a
promise that the generated API will become an official `sql-fun` feature.

Users may adopt, modify, or reject the example. In all three cases, feedback is
useful.

Especially valuable feedback includes why an application did not adopt the
example: an existing framework owns transactions, a different pool is required,
credentials or TLS are managed elsewhere, generated source is undesirable, or
the execution model is not connection-pool based.

The project does not promise to make every application accept one common
template. The objective is to identify patterns shared by enough users to justify
a reusable abstraction or stable extension point.

## Input Discovery for a Future Proc Macro

The example is also intended to discover which inputs would belong in a future
`#[connection_manager(...)]` attribute, if such an API is eventually justified.

Observed usage should help separate:

- values already known from datasource/project metadata
- explicit application choices that belong in proc-macro parameters
- connector-specific policy that should stay in a template
- application lifecycle decisions that must remain outside `sql-fun`
- reusable hooks or metadata that `sql-fun` should expose

Potential dimensions include connector selection, pooling, TLS, credentials,
runtime, timeout/retry policy, error mapping, observability, and transaction
context. Listing them here does not mean that each should become a proc-macro
parameter.

The goal is to derive the parameter surface from observed use rather than
prematurely freezing a broad configuration DSL.

## Application-Owned Lifecycle

Any generated API must preserve explicit application control over connection
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

The generator should emit ordinary application code and connector integration.
It should not require a new `sql-fun` runtime framework or global connection
registry.

Whether a selected connector or pool crate has its own runtime dependencies is a
property of that generated integration, not a new runtime dependency imposed by
the `sql-fun` query/codegen model itself.

## Rust 1.99 and a Possible Proc-Macro Surface

Rust 1.99 stabilizes custom attribute and derive macros around outlined modules
such as:

```rust
#[some_attribute]
mod generated;
```

That syntax provides a natural declaration point for a possible future
`#[connection_manager(...)] mod ...;` API.

It is not a prerequisite for exploring the generator itself. A Tera-based
example can generate the same module into a normal Rust source file on older
supported toolchains.

The current `sql-fun` MSRV is lower than Rust 1.99. A proc-macro API using this
syntax should only be considered once the project MSRV has independently moved
to Rust 1.99 or newer. This feature should not itself be used as the reason to
raise the MSRV.

## Open Questions

Areas to observe through the example and decide only if a formal API is later
considered include:

- which connector configuration belongs in project metadata
- whether pool integrations are templates, built-ins, or external extensions
- generated ownership and borrowing APIs for connections and transactions
- how async and synchronous connectors coexist
- TLS and authentication configuration boundaries
- error-type exposure
- support for execution contexts that are not conventional connection pools
- interaction with generated schema/query modules
- which choices should become proc-macro parameters versus template customization
