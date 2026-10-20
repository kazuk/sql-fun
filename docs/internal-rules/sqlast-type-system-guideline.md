# SQL AST Type System

This document is for implementers and reviewers of type resolution and
parameter inference in `sql-fun-sqlast`. Consult the existing representations
and their consumers before proposing another type model, catalog snapshot, or
inference mechanism. Keep this guide aligned with the source when changing
these contracts.

The [SQL analysis context guideline](sql-analysis-context-guideline.md) defines
the shared environment and semantic phase boundaries. It also applies here:
analyzers use `ParseContext`; pass-local inference state supplements it.

## Type references and definitions

The core representations are in
[`pg_type.rs`](../../sql-fun-sqlast/src/sem/pg_type.rs) and
[`pg_type/type_ref.rs`](../../sql-fun-sqlast/src/sem/pg_type/type_ref.rs).

| Representation | Responsibility |
| --- | --- |
| `TypeReference::Concrete` | Named type identity and an array qualifier; definition lookup uses the analysis context. |
| `TypeReference::Dynamic` | An owned, boxed clone of a `TypeDefinition`, including definitions without a registered schema name. |
| `TypeDefinition` | Enum, composite, domain, builtin, table-row reference, or implicit composite definition. |
| `TypeDefinition::ImplicitComposite` | Columns defining an anonymous row shape. |

`TypeReference::dynamic(&definition)` retains the definition independently of
the borrowed context. `type_definition()` exposes that owned definition and
returns `None` for concrete references. Dynamic is an ownership/representation
choice, not a synonym for an unresolved parameter or an unrestricted domain.
Use it for anonymous types as well as definitions retained from the catalog.

Ownership is not recursive catalog capture. Fields may still contain named
references, and `TypeDefinition::TableRef` refers to a table whose columns live
in the catalog. Determine which definitions the consumer actually needs.
For example, member projection materializes table columns into an
inference-owned `ImplicitComposite` without modifying the shared catalog model.

Before adding a snapshot or a second owned representation, check whether
`Dynamic` and the existing definition variants meet the lifetime requirement.
Keep processing-specific relationships in the processing module. Preserve the
source model and serialization contract; a redesign requires its own agreed
scope.

### Operation boundaries

Do not assume every `TypeReference` method supports both variants. Several
dynamic branches currently use `todo!()`, including `full_name()`, display,
`element_type()`, and polymorphic-type operations. Inspect the method before
using it on a potentially dynamic reference. Concrete canonical text parsing
and formatting do not provide a text representation for dynamic definitions.

The concrete array qualifier is a boolean, not a dimension count. Do not infer
multidimensional SQL indexing semantics from that representation alone. The
structural array projections currently operate on concrete array references.

## Known types, unknowns, and casts

Expression typing and nullability are separate results: `get_type()` returns
an optional type reference, while `is_not_null()` returns optional nullability.
An absent type does not by itself establish an error, SQL NULL, or a concrete
fallback type. Check the expression and resolution phase that produced it.

Use the existing catalog lookup and cast interfaces through `ParseContext` /
`BaseParseContext`. Resolve SQL names with the active search path and dialect;
canonical concrete type text parsing is not SQL alias resolution.

[`cast.rs`](../../sql-fun-sqlast/src/sem/cast.rs) distinguishes identity,
implicit, assignment, and explicit conversions. A catalog cast's existence
does not make it legal in every expression position. Check the required
conversion context; array bounds, for example, use assignment coercion to
`int4`. Verify SQL behavior against the PostgreSQL development oracles when
changing conversion or inference rules, including version differences where
relevant. These servers are development checks, not ordinary test dependencies.

[`type_system.rs`](../../sql-fun-sqlast/src/sem/type_system.rs) contains function
argument binding and simple/common polymorphic inference.
[`overload_rank.rs`](../../sql-fun-sqlast/src/sem/overload_rank.rs) compares exact
matches before preferred conversions. Inspect these existing paths and their
callers before adding another binding or ranking strategy; the ranking helper
alone is not a complete implementation of PostgreSQL overload resolution.

## Deferred parameter constraints

[`parameter_resolution.rs`](../../sql-fun-sqlast/src/sem/parameter_resolution.rs)
collects constraints from the name-resolved semantic tree and finalizes results.
[`parameter_constraints.rs`](../../sql-fun-sqlast/src/sem/parameter_constraints.rs)
owns domains, correlated alternatives, and the common propagation graph.

- `InputTypeConstraint::any()` means unrestricted, not inference failure.
  A finite domain with no candidates represents an impossible constraint.
- Per-parameter domains cannot express all relationships. Use
  `ParameterConstraints::allows` to inspect whether supplied types can coexist;
  checking each domain independently can accept incompatible combinations.
  Omitted positions remain unconstrained by that inspection; unknown parameter
  positions are rejected. Inspection does not supply analysis hints.
- Preserve structure for unresolved expressions in the semantic tree and graph.
  Array indexing, slices, and member access use relationships in
  [`parameter_constraints/structural.rs`](../../sql-fun-sqlast/src/sem/parameter_constraints/structural.rs)
  so later information can constrain both source and result. A member result
  alone does not enumerate every possible catalog composite.
- Constraint results outlive the borrowed analysis environment. Member
  projections retain the definitions encountered during analysis using owned
  references. When inspecting a previously unknown composite with `allows`,
  supply a dynamic reference containing its definition; a new concrete name
  alone cannot provide its fields. This is not a catalog snapshot API.

Diagnostics must remain specific when a type becomes known through another
expression or projection. Propagation can discard invalid candidates; inspect
the relevant state before losing the evidence needed for a structured problem.
Use the existing propagation path rather than a second diagnostic solver.
Regression coverage should check the diagnostic identity as well as inferred
domains and retained expression structure, including nested deferred failures.

## Test context selection

Follow the [Rust test placement and helper cost rules](rust-coding-guideline.md).
Choose the environment from the behavior under test:

- For a small set of types, casts, and operators, use the existing unit-test
  `TestParseContext` and explicit definitions. Where real DDL application and
  freeze behavior matter, use that small base with `SchemaFileContext`. See
  [`indirection_tests/test_helpers.rs`](../../sql-fun-sqlast/src/sem/parameter_resolution/indirection_tests/test_helpers.rs).
- Use `test_helpers::heavy` when real builtin catalog loading or catalog-wide
  interactions are themselves needed for the assertion. State that reason and
  preserve the visible heavy call path. A query using an integer cast and a
  few local definitions does not inherently require the builtin catalog.

Model the conversions and signatures the test needs explicitly. A minimal stub
does not prove real catalog completeness; retain integration coverage where
that is the contract. Process-local caching does not share setup across
nextest's independent test processes. Documenting heavy setup cost alone does
not establish that a particular test needs it.
