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
For example, member projection materializes table and domain-base row columns
into an inference-owned `ImplicitComposite`, keyed by the original input type,
without modifying the shared catalog model. Member lookup follows nested domain
bases while retaining the source and declared field identities. Arrays remain
non-composite values; unresolved or cyclic bases do not establish a row shape.
Owned domain definitions can also project entirely owned row fields without a
live catalog.

Before adding a snapshot or a second owned representation, check whether
`Dynamic` and the existing definition variants meet the lifetime requirement.
Keep processing-specific relationships in the processing module. Preserve the
source model and serialization contract; a redesign requires its own agreed
scope.

### Operation boundaries

`TypeReference::full_name()` exposes optional nominal identity. Concrete
references return their name; owned named definitions return their retained name.
Anonymous composites and structural arrays have no standalone catalog name.
`TypeDefinition::full_name()` and its borrowed `type_reference()` return `None`
for implicit composites. Use the retained definition and row/structural APIs
instead of fabricating a `record` binding or sending anonymous values through
catalog-key lookup. Canonicalization preserves owned definitions.

Display is diagnostic text: anonymous rows show ordered field types as
`row(type, ...)`, including `?` for an unresolved field, and structural arrays
append `[]`. This is not a canonical SQL spelling or a dynamic `FromStr`
round-trip contract. Concrete canonical parsing/formatting remains unchanged.
The server query-result type payload remains nominal-only; its absent type field
does not erase the SQLAST's owned fields.

Dynamic rows/arrays are not polymorphic placeholders or ranges. Owned nominal
references reuse the represented type's predicates. Simple/common substitution
retains the complete inferred element or array definition; fixed types remain
unchanged. Range/multirange construction and subtype metadata extraction remain
unfinished in the existing common nominal implementation and are tracked by
[#914](https://github.com/kazuk/sql-fun/issues/914). They are not implemented by
optional name access or anonymous structural matching. Extraction requires the
corresponding range receiver, just as element extraction requires an array.

`TypeReference::array_type()` constructs either a concrete array qualifier or a
dynamic `TypeDefinition::Array`. On a dynamic array, `element_type()` returns the
retained element reference; calling it on a non-array dynamic definition panics.
For semantic array applicability, use the context's `get_array_item_type`:
PostgreSQL `int2vector` and `oidvector` have elements even though their concrete
array qualifier is false. Their names remain distinct from `int2[]` / `oid[]`
and from arrays of vectors. Simple-family inference retains the actual array
argument identity as well as its element; common-family substitution constructs
the ordinary array of its chosen common element. Vector-source coercion to an
ordinary array follows the element conversion context; the reverse conversion
is not synthesized.

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

## Type collation facts

`TypeReference::collation_capability` returns the retained PostgreSQL capability
through `ParseContext`. Existing nominal definitions retain `collatable:
Option<bool>`; domains and ordinary arrays follow their existing base/element
references, including owned dynamic definitions. Nominal vectors use their own
fact. Undefined facts, unresolved references, cycles and other dialects return
`None`. The query performs no supplementary discovery and emits no diagnostics;
SQL consumers own applicability and diagnostic policy. See the
[extraction contract](postgres-extraction.md#type-collation-capability) for
required JSON-key presence and explicit null semantics.

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

Schema catalog implicit coercion reduces source and target domain layers using
`TypeReference::domain_base_type`, without rewriting the original references.
A domain coercion remains a conversion even when its bases are identical;
`NoConversion` describes original identity. Preferred-conversion credit belongs
to the original target: source-domain reduction retains a preferred builtin
target, while target-domain reduction receives ordinary implicit credit.
Operator ranking first preserves
complete original exact signatures, then scores domain inputs as their bases.
The same `OverloadRank::operator_argument` facts feed direct and deferred
operator selection. Domain/unknown literals try the exact domain signature
before the base signature. Cast eligibility and target enumeration share the
catalog owner; declared source enumeration still does not discover domains to
expand bounded parameter defaults.

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
ordered common-type selection in `CommonType` without requiring GREATEST/LEAST's
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

### ARRAY query results

`ArraySubqueryExpr` retains an analyzed one-column query separately from the
operand-list `ArrayExpr`. It constructs a non-null array, including empty
results and nullable scalar elements. Array-valued columns retain the same
PostgreSQL array type identity; runtime accumulation restrictions do not justify
rejecting their static types. A pending star projection defers the width check
until parameter finalization can expand it. The existing construction projection
keeps both scalar and array-valued source alternatives when an enclosing result
constraint is propagated backwards. Keep the result node stable across expansion
so enclosing operations retain their graph links. Invalid widths retain the
query, an empty result domain, and an `array_subquery_columns` diagnostic.

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
- Domains retain admissible source candidates, with bounded defaults for
  unhinted overloads as described below, rather than the single parameter
  signature inferred by an untyped PostgreSQL `PREPARE`. For example,
  `SELECT ARRAY[$1,1]` admits both an `int4` input with an `int4[]` result and an
  `int8` input with an `int8[]` result; an untyped PostgreSQL PREPARE selects
  `int4`. Application binding selection is retained separately from SQL admissibility,
  as described below.
  `ParameterConstraints::result_column` exposes the output domain;
  `col_type()` may remain absent when multiple result types survive. An explicit
  SQL cast can narrow the result. See the public-boundary tests in
  [`tests/arrays.rs`](../../sql-fun-sqlast/tests/arrays.rs) and the
  [parameter analysis documentation](../../book/sql-analysis/expressions/parameters.md).
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

### Application binding selection

Semantic-analysis SQL already contains numbered placeholders. Names and optional
resolved hints are associated with those numbers by the existing parameter input
contract. Do not reconstruct the original named/native syntax or collect hints
by occurrence inside SQLAST. A supplied hint is authoritative.

For an unhinted application input, use its first syntactic SQL occurrence,
identified by retained parser source positions rather than semantic traversal
order. The nearest consuming operation supplies local admissibility and identity
facts for the seven default input types. A unique identity candidate wins;
otherwise a unique admissible candidate wins. Multiple candidates remain
ambiguous. Subsequent occurrences validate that fixed choice; they cannot
resolve an ambiguous first use or revise a selected type. Candidate domains
must be bounded at construction, never filtered in diagnostic rendering.

Keep application decisions separate from the SQL constraint graph and its
`allows` inspection contract. `ParameterConstraint::binding_type` exposes the
application decision; `constraint` continues to describe SQL admissibility.
Application result typing uses the existing graph with selected bindings applied,
while inspection retains the original SQL requirements. An independently selected
binding supplies evidence for operation-specific diagnostics, including an
inapplicable operator at a later use. Report selected types and conflicting
requirements without erasing the binding.
When deferred projection expansion resumes collection, parameter occurrences
must supply their retained SQL input constraints, never their finalized
application types. Keep that transient input state out of serialized expressions
and grouping identity.

### Default candidates and deferred overloads

Overload constraints retain declarations and owned cast ranks in
`parameter_constraints/call.rs`. Initially unrestricted calls wait for finite
operand domains from other occurrences, casts, common-type selection, and
structural projections. A sole concrete declaration supplies SQL type context.
Finite declaration requirements that cannot accept the defaults (such as
`age`'s datetime overloads) propagate before any default alternatives.
Only remaining unseeded parameter-placeholder inputs use the seven built-in
default bind candidates; graph node identities preserve their semantic origin.
Candidate order never chooses a binding. Local application selection follows
the separate rule above; an absent default witness does not establish invalid SQL.

The same bound applies to deferred common-type relationships. Finish SQL-derived
propagation before preparing finite witnesses, including equality and structural
projection constraints. Only unseeded placeholder inputs receive the seven
candidates; an unresolved expression node does not inherit them. Common-type
facts are prepared for encountered types instead of enumerating the catalog.
Entering the default phase does not authorize adding default witnesses to a
relation already seeded by SQL-established types.
Keep the bounded-default phase through nested propagation: a result derived
from defaults must not expand another placeholder back to catalog cast sources.
The original effective catalog is retained through `ParseContext::retain_catalog`:
the immutable base is shared and the active layer is copied, without retaining
diagnostics or inferred graphs. The original analysis arguments and metadata
remain explicit when constructing the later inspection context.

`allows` restores the pre-default graph and prepares the original semantic facts
on demand for explicit types, including non-default nominal types, domains,
arrays and supplied owned definitions. Missing prepared facts are not evidence
of an illegal assignment. Provisional solves and resumed collection retain this
capability and must not publish speculative defaults as SQL constraints.

Keep `anyarray` declarations deferred. Instantiate comparison signatures from
observed array types and their conversion targets, preserving concrete overload
precedence. Retain conversion ranks only for finite declaration/default types
and types encountered in SQL; inspect casts for those specific sources or
targets. Do not call `type_references()` or cache the catalog's type/cast
universe as deferred-overload inference preprocessing.

An unresolved non-placeholder expression must not inherit placeholder defaults.
When its polymorphic relationship would require an unbounded type universe,
report recoverable `inference_candidate_space_unsupported` after propagation.
Deliver it only after statement finalization, since provisional RETURNING and
projection solves may acquire type context in a later collection pass.
Keep its graph nodes and unresolved domains, preserve known result metadata,
and do not manufacture an empty domain or `parameter_constraint`: unsupported
inference is not evidence of invalid SQL. Explicit assignments may still resolve
those nodes through retained relationships.

Unhinted overload domains expose default inference candidates, not a restriction
on explicitly supplied client types. A solved graph retains its pre-solve SQL
constraints for `allows`. Explicit
assignments are checked against those constraints before default witnesses are
supplied for remaining inputs. Resuming constraint collection restores that
original graph so earlier default candidates cannot exclude later SQL type
information. Keep unrestricted, empty, and finite domains distinct; preserve
correlations and the shared diagnostic boundary.

Diagnostic attribution must follow the available evidence. When independently
established operand types violate an operation's requirements, report the
operation-specific structured problem (for example, array access on a known
scalar). When no application input type has been selected and multiple
requirements on
an unresolved parameter conflict, the solver cannot determine which expression
is erroneous. Report the unsatisfiable SQL constraints without arbitrarily
blaming one operation. Once an input type has independently been selected by
an explicit hint or first-occurrence local selection, validate operations with
that fixed input and report their applicable structured diagnostics. Do not
use later SQL constraints to revise or manufacture the binding decision.

Do not preserve discarded candidates solely to manufacture operation-specific
errors from otherwise unattributable conflicts. Constraint satisfaction and
`ParameterConstraints::allows` remain independent of diagnostic presentation.
See the maintainer decision in [#683](https://github.com/kazuk/sql-fun/issues/683#issuecomment-6324548856).
Regression coverage should verify diagnostic identities, admissible assignments,
and the absence of falsely attributed errors.

## Test context selection

Follow the [Rust test placement and helper cost rules](rust-coding-guideline.md).
Choose the environment from the behavior under test:

- For a small set of types, casts, and operators, use the existing unit-test
  `TestParseContext` and explicit definitions. Where real DDL application and
  freeze behavior matter, use that small base with `SchemaFileContext`. See
  [`test_helpers/indirection.rs`](../../sql-fun-sqlast/src/test_helpers/indirection.rs).
- Use `test_helpers::heavy` when real builtin catalog loading or catalog-wide
  interactions are themselves needed for the assertion. State that reason and
  preserve the visible heavy call path. A query using an integer cast and a
  few local definitions does not inherently require the builtin catalog.

Model the conversions and signatures the test needs explicitly. A minimal stub
does not prove real catalog completeness; retain integration coverage where
that is the contract. Process-local caching does not share setup across
nextest's independent test processes. Documenting heavy setup cost alone does
not establish that a particular test needs it.

## Explicit declaration collations

`DeclarationCollation` consumes a normally resolved type and a completed
`CatalogNameLookup`; it never resolves a type or discovers additional metadata.
Record-returning function columns, table/add-column declarations, composite
attributes and domain bases share this operation. Semantic declarations retain
requested types/modifiers separately from canonical type identities, and retain
collation lookup failures as well as the applicability outcome.

A missing type takes diagnostic precedence over collation lookup failure; a
missing or ambiguous collation takes precedence over capability validation.
Failed lookup must not manufacture a non-collatable-type diagnostic. A true
capability permits explicit COLLATE; false reports `collate_operand_type`.
An undefined capability reports recoverable `collate_capability_undefined` and
retains an indeterminate outcome. It is neither evidence of non-collatability
nor permission to publish an unvalidated declaration. A null fact alone does
not distinguish a PostgreSQL shell from another incomplete catalog reference.
Omitted COLLATE does not select a collation or request capability validation.

DDL application consumes retained declaration applicability before catalog
mutation, independently of diagnostic reporting or rendering. Invalid table,
composite and domain declarations publish no definitions; invalid add-column
groups leave the table unchanged. Query declarations preserve known result
columns even when collation validation fails.

Successful PostgreSQL ADD COLUMN application remains tracked by
[#528](https://github.com/kazuk/sql-fun/issues/528); sharing declaration validation
does not implement that operation. ALTER COLUMN TYPE COLLATE retains syntax but
its semantic operation remains owned by
[#529](https://github.com/kazuk/sql-fun/issues/529). Expression COLLATE has its own
validation lifecycle; this declaration policy does not change it or SQLite.
