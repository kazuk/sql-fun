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
| `TypeDefinition` | Array, enum, composite, domain, builtin, table-row reference, or implicit composite definition. |
| `TypeDefinition::Array` | An `ArrayTypeDefinition` retaining its element reference, including anonymous structural definitions. |
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
and some polymorphic-type operations. Inspect the method before using it on a
potentially dynamic reference. Concrete canonical text parsing and formatting
do not provide a text representation for dynamic definitions.

`TypeReference::array_type()` constructs either a concrete array qualifier or a
dynamic `TypeDefinition::Array`. On a dynamic array, `element_type()` returns the
retained element reference; calling it on a non-array dynamic definition panics.
`ArrayTypeDefinition::element()` exposes the element by reference. See
[`pg_type/array.rs`](../../sql-fun-sqlast/src/sem/pg_type/array.rs).
An already-array reference stays the same when `array_type()` is called again.
The concrete array qualifier is a boolean, not a dimension count: nested SQL
arrays share their element family's array type identity. Do not infer SQL
indexing or shape validation from this representation alone. The deferred
structural array projections described below still require concrete references;
support for dynamic array construction and element introspection does not imply
support in every inference consumer.

Simple/common polymorphic substitution can use a dynamic inference result for
the supported element, array, and non-array placeholders (`anyelement`,
`anyarray`, `anynonarray`, and their `anycompatible` counterparts). This does not
make all polymorphic methods callable on a dynamic receiver.

## Known types, unknowns, and casts

Expression typing and nullability are separate results: `get_type()` returns
an optional type reference, while `is_not_null()` returns optional nullability.
An absent type does not by itself establish an error, SQL NULL, or a concrete
fallback type. Check the expression and resolution phase that produced it.

Use the existing catalog lookup and cast interfaces through `ParseContext` /
`BaseParseContext`. Resolve SQL names with the active search path and dialect;
canonical concrete type text parsing is not SQL alias resolution.
The crate-private `TypeReference::definition_from_catalog` returns the retained
dynamic definition or looks up a concrete reference through the shared context.

If either operand is dynamic, it cannot be passed through the catalog cast
master. Use the existing `TypeReference::implicit_match_context` path, shared by
overload ranking, argument validation, and common-type coercion checks. It
compares structure instead of querying cast masters; only two concrete operands
use catalog implicit-cast lookup. Structural matching recursively compares array
elements and positional anonymous-record field types, ignoring field names and
nullability. Named composites retain nominal identity: matching field layouts
do not authorize implicit conversion from an anonymous record to a named
composite or its array. An anonymous record can satisfy a generic `record`
argument. `NoConversion` records a successful match in these callers; verify
PostgreSQL-visible candidate selection and rejection behavior when changing it.
Keep these rules in the existing matching implementation rather than adding a
second cast service.

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
Function ranking retains the original call-bound declarations alongside the
instantiated bindings. Instantiated inputs establish applicability and result
types; substituted polymorphic positions earn no exact-match or preferred-cast
credit. This distinction also survives in deferred function signatures, where
ranking is evaluated separately for each admissible source assignment. Operator
signatures keep their existing ranking behavior. For example, `fa(control[])`
beats `fa(anyarray)` for `fa(NULL::control[])`, and `f(record)` beats
`f(anyelement)` for an anonymous record. Structural matching still determines
applicability without catalog cast lookup for dynamic operands; successful
structural matching alone does not establish the correct overload choice.
See the regressions for [#673](https://github.com/kazuk/sql-fun/issues/673) in
`function_overload_collection/tests.rs` and `tests/arrays.rs`.

### ARRAY constructor results

[`scalar_expr/array.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/array.rs)
retains every operand, output type domains, and element coercions. It reuses the
ordered common-type selection in `MinMaxExpr` without requiring GREATEST/LEAST's
comparison implementation. An explicit array cast supplies the element target
to nested and empty constructors before analysis. Constructor-specific typing
failures retain an empty domain and structured `ArrayProblem` diagnostics rather
than becoming unrestricted inputs. Construction is non-null independently of element
nullability.

Anonymous element definitions can remain available through the structural array
API. SQL subscripting of an anonymous-record array nevertheless yields generic
`record` without accessible field names; retained internal fields do not imply
that subsequent SQL member access is legal. See
[`tests/arrays.rs`](../../sql-fun-sqlast/tests/arrays.rs) for the constructor,
parameter, coercion, and record-overload regression coverage.

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
- These domains represent admissible explicitly typed inputs, not the single
  parameter signature inferred by an untyped PostgreSQL `PREPARE`. For example,
  `SELECT ARRAY[$1,1]` admits both an `int4` input with an `int4[]` result and an
  `int8` input with an `int8[]` result; an untyped PostgreSQL PREPARE selects
  `int4`. Analysis does not choose a final wire-protocol binding.
  `ParameterConstraints::result_column` exposes the output domain;
  `col_type()` may remain absent when multiple result types survive. An explicit
  SQL cast can narrow the result. See the public-boundary tests in
  [`tests/arrays.rs`](../../sql-fun-sqlast/tests/arrays.rs) and the
  [parameter analysis documentation](../../book/sql-analysis.md#parameter-analysis).
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
