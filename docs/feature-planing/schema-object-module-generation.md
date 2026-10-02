# Schema Object Module Generation

*This is a planning document outlining a potential future feature of
`sql-fun`.
It is not yet implemented, and no timeline or commitment has been established.*

## Motivation

`sql-fun` already treats database schema metadata as a source of truth for SQL
analysis and Rust code generation.

A module-level procedural macro would allow a complete Rust namespace to be
projected from one datasource in a single declaration instead of requiring
individual generation requests for each table or other schema object.

Conceptually:

```rust
#[schema_objects(datasource = "main")]
mod main_db;
```

could generate a module containing the Rust representation of schema objects
available from the configured datasource.

Possible generated objects include:

- tables and their row/input types
- views
- database-defined enum or domain types
- functions and procedures where supported
- schema-qualified namespaces
- generated query or mutation helpers derived from those objects

The exact generated API is intentionally not fixed by this planning document.

## Namespace Model

The module provides a natural namespace boundary for one datasource:

```rust
#[schema_objects(datasource = "main")]
mod main_db;

#[schema_objects(datasource = "analytics")]
mod analytics;
```

This allows otherwise identical database object names to remain distinct through
ordinary Rust paths:

```text
main_db::public::users
analytics::public::users
```

Database namespaces such as PostgreSQL schemas can be projected below the
datasource module where appropriate.

The important property is that the schema inventory remains the source of truth.
The generated module is a Rust projection of that inventory rather than a second
independent schema definition.

## Relationship to Existing Code Generation

This feature would complement narrower generation mechanisms such as
`from_table` rather than requiring them to disappear.

A module-level macro is useful when the desired generation unit is the datasource
schema as a whole. Existing item-level or function-like APIs remain useful when
an application intentionally wants only selected generated items.

All generated objects should remain compile-time output. This proposal does not
introduce a runtime schema registry or a runtime dependency on `sql-fun`.

## Rust Version Requirement

Rust 1.99 stabilizes custom attribute and derive macros around outlined modules
such as:

```rust
#[some_attribute]
mod generated;
```

This makes an outlined module a practical declarative insertion point for
module-wide generated items.

The current `sql-fun` MSRV is lower than Rust 1.99. Therefore this feature is
planning-only while that remains true.

Implementation should only be considered after the project MSRV has independently
moved to Rust 1.99 or newer. This feature should not itself be used as the reason
to raise the MSRV.

## Open Questions

Areas to decide during implementation planning include:

- which schema object kinds are generated initially
- how SQL schemas/catalogs map to nested Rust modules
- include/exclude filtering
- identifier normalization and collision handling
- visibility of generated items
- interaction with existing generated query APIs
- how datasource metadata selects the schema inventory used for expansion
