# Rust Semantic Artifact for Cross-Domain Static Analysis

## Motivation

`sql-fun` can describe the semantics of SQL queries, but some useful diagnostics
require semantic information from the host Rust program as well.

A simple example is detecting an ad-hoc query executed directly inside a loop. A
more useful analysis goes beyond lexical loop detection and follows the Rust
call graph from an application boundary such as a domain service:

```text
domain_service
├── validate
│   └── Query A [ad-hoc]
└── load_detail
    └── Query A [ad-hoc]
```

Even without a syntactic loop, the same query may be reachable from multiple
call sites during one operation. In such a case, preparing the query once and
passing the prepared query through the call graph may be preferable.

Implementing a Rust call-graph and type-resolution engine inside `sql-fun` would
duplicate work already performed by `rustc` and would couple `sql-fun` to
unstable compiler internals.

## Idea

Explore a compiler-side tool, driver, or backend-like component that emits a
**semantic artifact** instead of a target binary artifact.

Conceptually:

```text
Rust source
    ↓
rustc parsing / name resolution / type checking / trait resolution
    ↓
semantic artifact
```

The artifact would expose enough resolved program information for downstream
static-analysis tools without requiring each tool to parse Rust source and
reconstruct compiler semantics independently.

Potential information includes:

- stable or externally usable item identity
- functions, methods, closures, and source spans
- resolved call edges
- resolved trait and method targets
- generic instantiations where useful
- async and closure boundaries
- attributes and other semantic markers
- repeated-execution contexts such as loops or iterator-driven closures, when
  representable

This should be considered a semantic interoperability artifact rather than a
binary/code-generation artifact.

## sql-fun Integration

`sql-fun` could independently provide query semantics such as:

```text
query identity / fingerprint
query kind: ad-hoc / prepared
source location
datasource
schema dependencies
```

A downstream analyzer could then join the Rust semantic artifact with the
`sql-fun` semantic artifact:

```text
Rust semantic graph
        +
SQL semantic graph
        ↓
application semantic graph
```

Possible diagnostics include:

- an ad-hoc query executed in a loop or iterator repetition context
- the same ad-hoc query reachable from multiple call sites under one
  domain-service boundary
- repeated preparation where a prepared query could be created once and passed
  downward
- prepared query construction outside the useful ownership scope
- query execution paths multiplied by application control flow
- resource-usage analyses that combine database operations with other expensive
  operations

The important property is that `sql-fun` does not need to own a Rust compiler
analysis engine. It only needs to expose its own semantic facts in a form that
another analyzer can correlate with Rust compiler facts.

## Scope

This is not currently a commitment to implement a custom `rustc` backend or
compiler driver inside `sql-fun`.

Possible implementations may live:

- inside `sql-fun`
- as a separate Rust compiler tooling project
- as an integration with an existing static-analysis or call-graph tool
- as an upstream experiment around a stable semantic artifact emitted by Rust
  tooling

The preferred long-term outcome is an ecosystem-level semantic artifact that can
be reused by tools beyond `sql-fun`, rather than every domain tool maintaining
its own Rust semantic analyzer.
