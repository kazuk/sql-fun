# Schema Object Module Generation

*This is a planning document outlining a potential future feature of
`sql-fun`.
It is not yet implemented, and no timeline or commitment has been established.*

## Motivation

`sql-fun` already treats database schema metadata as a source of truth for SQL
analysis and Rust code generation.

A complete Rust namespace can be projected from one datasource using the schema
inventory and project metadata as generator input.

A possible future module-level procedural macro API is:

```rust
#[schema_objects(datasource = "main")]
mod main_db;
```

Such a module could contain Rust representations of schema objects available
from the configured datasource.

Possible generated objects include:

- tables and their row/input types
- views
- database-defined enum or domain types
- functions and procedures where supported
- schema-qualified namespaces
- generated query or mutation helpers derived from those objects

The exact generated API is intentionally not fixed by this planning document.

## Example-First Exploration

Before committing to a procedural-macro API, provide the same idea as an
example-side generator using schema inventory/project metadata and a Tera
template.

Conceptually:

```text
schema inventory + project metadata
                 |
                 v
             Tera template
                 |
                 v
       generated Rust module
```

The template should live under `examples/` and be linked from this planning
document. It is an example, not a supported public API or a commitment that
`sql-fun` will adopt the generated shape as an official feature.

Users may:

- use the template as-is
- modify it for their own conventions
- copy only parts of it
- reject the approach entirely

All of those outcomes are useful feedback.

In particular, reasons for not adopting the example should be treated as
use-case information. The project does not promise to eliminate every rejection
reason. The goal is to understand which parts generalize across more users and
which parts should remain application-specific.

## Input Discovery for a Future Proc Macro

The example is also intended to discover the parameter surface of a possible
future proc-macro API.

Observed user changes can help distinguish among:

- values that should come automatically from schema inventory or project metadata
- decisions that should become explicit proc-macro parameters
- conventions that belong only in a user-owned template
- extension points that should be exposed by `sql-fun`
- concerns that should not become part of the feature at all

This avoids defining an attribute parameter list from hypothetical use cases
before real generator usage exists.

For example, candidate dimensions may include schema/catalog selection,
include/exclude filters, object kinds, identifier conventions, visibility, type
mapping, and module nesting. Their presence here does not imply that each should
become a future proc-macro parameter.

## Namespace Model

A future module API would provide a natural namespace boundary for one
datasource:

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

The implemented query macros (`sql_query_one`, `sql_query_opt`, `sql_query_many`,
and `sql_statement`) generate functions from SQL. This proposal instead concerns
Rust representations of the datasource schema as a whole.

The former `from_table` attribute was a pass-through stub, not an implemented
table/view field generator, and has been removed. This proposal does not depend
on that attribute or schedule a replacement for it.

The example generator can exist independently of a proc-macro implementation.

## Rust 1.99 and a Possible Proc-Macro Surface

Rust 1.99 stabilizes custom attribute procedural macros on outlined modules
such as:

```rust
#[some_attribute]
mod generated;
```

That syntax is a natural surface for a future inline API such as
`#[schema_objects(...)] mod ...;`.

It is not a prerequisite for exploring or implementing the generator itself.
Equivalent generation can already be performed externally, for example by
rendering a Tera template into a module file.

The current `sql-fun` MSRV is lower than Rust 1.99. A proc-macro API using this
syntax should only be considered once the project MSRV has independently moved
to Rust 1.99 or newer. This feature should not itself be used as the reason to
raise the MSRV.

## Open Questions

Areas to observe through the example and decide only if a formal API is later
considered include:

- which schema object kinds are broadly useful to generate
- how SQL schemas/catalogs map to nested Rust modules
- include/exclude filtering
- identifier normalization and collision handling
- visibility of generated items
- interaction with existing generated query APIs
- how datasource metadata selects the schema inventory used for expansion
- which choices should be parameters versus template customization
