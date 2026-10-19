# Loaded type values and legacy API migration

This document is for maintainers adopting `sql_fun_sqlast::types`. It records the
API boundary and remaining work when moving a semantic consumer to owned loaded
types. It is internal API design documentation: this PR does not change CLI
behavior or require additions to the user manual.

## Module boundary

`sem::TypeDefinition`, `sem::TypeReference`, and `ParseContext` remain the existing
semantic storage and analysis interfaces, including their serialization contracts.
The new `types::TypeDefinition` is a trait for an owned loaded value;
`types::TypeReference` is `Box<dyn TypeDefinition>`. Category implementations own
their components and do not retain legacy declaration or column-collection
payloads. `sem::FullName` and `sem::ColumnName` remain shared name values.

Only `types::compat` interprets legacy definitions and references. Adoption is
explicit: existing consumers do not begin loading types simply because the new
module exists. Catalog-dependent loading takes the consumer's `ParseContext`;
there is no replacement catalog or ambient context discovery.

The shared trait exposes `as_any`, `clone_box`, and `same_type`. `Clone` on the
boxed reference and `PartialEq`/`Eq` on the trait object use those operations.
Category inspection uses `as_any().downcast_ref::<Category>()`; the trait does not
pretend that every type has a name, element, base, or members. Accessors expose
only the category's components. A consumer can inspect a loaded value without
consulting storage again.

## Selected-provider type entrypoint

`types::TypeContext::new(&provider)` is the public entrypoint for consumers
working with loaded values. `TypeContext` and the public `TypeContextRead`
provider contract live permanently in `types/context.rs` and are re-exported
from `types`. The facade borrows an explicitly selected provider and delegates
catalog facts; it owns no catalog storage and discovers no ambient inputs.
PostgreSQL coercion policy operates on loaded components in
`types/cast/resolve.rs`, separately from the context entrypoint, provider and
removable adapter.

`TypeContextRead` has an associated `Error` and eight primary loaded-value
operations:
two type lookups, three directed cast lookups and three candidate enumerations.
Native providers implement this contract directly using their own error type,
without importing legacy references or `types::compat`. Removing the temporary
legacy adapter later leaves the public `TypeContext` implementation and provider
interface intact. The required `dialect` method returns the actual selected
`ast::Dialect`. PostgreSQL rules follow that dialect, without a separate feature
flag or silent default. Other dialects retain provider-only cast behavior.
`registered_cast` supplies an actual native catalog entry, while
`existing_array_cast` supplies a provider's existing whole-array relationship.
Both fact hooks default to no fact and contain no derivation policy.

`types/compat/context.rs` contains the blanket adapter implementing
`TypeContextRead` for a selected `sem::ParseContext`, with
`type Error = compat::ConversionError`. All legacy projection, conversion and
catalog fact retrieval stays in `compat`. Its `dialect` method forwards
`ParseContext::dialect`; it implements no component coercion rules.
Existing consumers can therefore pass
their selected `ParseContext` to `TypeContext::new`, while native providers have
no dependency on that adapter.

`ParseContext` remains the common environment passed through SQL analyzers and
the authority for their catalog, dialect, search path and analysis configuration.
The explicitly requested `TypeContext` facade provides loaded-type reads; it is
not an alternative analyzer environment. This addition does not migrate existing
analyzers or replace their `ParseContext` parameters.

| Operation | Loaded API and behavior |
| --- | --- |
| Resolve a selected type name | `type_by_name(&FullName)` follows the selected provider's type search path and returns `Result<Option<TypeReference>, C::Error>`. |
| Read an exact catalog key | `type_by_exact_name(&FullName)` performs exact lookup without search-path expansion, with the same result shape. |
| Inspect an array component | `array_item_type(&dyn TypeDefinition)` returns an owned clone of an `ArrayType` element, or `None` for another category. It requires no provider trait bound and performs no catalog lookup. |
| Look up a directed relationship | `implicit_cast`, `assignment_cast`, and `explicit_cast` accept loaded source and target values and return `Result<CastQueryOutcome<Option<CastDefinition>>, CastQueryError<C::Error>>`. |
| Enumerate source candidates | `cast_sources(target, explicit)` and `assignment_sources(target)` accept a loaded target and return a loaded `Candidates` or `Never` through `Result<CastQueryOutcome<TypeReference>, CastQueryError<C::Error>>`. |
| Enumerate implicit targets | `implicit_castable(source)` accepts a loaded source and returns the selected provider's implicit target candidates through the same result shape. |

Here `C` is the selected provider implementing `TypeContextRead`. Name lookup
distinguishes an absent definition (`Ok(None)`) from a present definition whose
components cannot load (`Err`). Array inspection is intrinsic: an `ArrayType`
containing `Any` yields a cloned `Any` element. Nested arrays are preserved;
a scalar returns `None`; a domain is not flattened to expose its underlying
array. These operations do not depend on legacy representability.

### Temporary legacy adapter

The following projection restrictions belong to the `ParseContext` adapter,
not to `TypeContextRead` or to native providers. Provider operations delegated
to the legacy catalog project loaded inputs
faithfully into legacy references. Permanent structural operations use their
loaded components directly and need no legacy array flag or catalog
registration. A nominal name alone is insufficient: projection reloads its
definition from the selected catalog and compares the complete loaded snapshot,
including category and recursively loaded components. A stale or different
snapshot returns `LoadedTypeMismatch` rather than discarding labels, members,
base types or other identity components. Array projection retains the wrapper
and validates the element snapshot.

Anonymous structural values can project into dynamic legacy definitions when
all components have faithful representations. Delegation preserves the legacy
catalog's handling of dynamic references, including its rejection of dynamic cast
lookups; projection does not invent structural cast rules. Direct low-level
legacy conversion of constraints (`Any`, `Never`,
`Candidates`), custom trait implementations, or nested arrays without a faithful
legacy representation returns `UnsupportedLoadedType`. For optional provider
facts, a value without a legacy reference contributes no catalog fact; this does
not invalidate the loaded type or stop permanent component derivation. The
adapter does
not convert them into nominal placeholders, flatten them or expand them into new
searches. These restrictions do not limit owned construction, equality, cloning,
intrinsic array inspection or a native provider's supported loaded categories.

The low-level `compat::load_implicit_castable(context, source)` exposes the
legacy implicit-target enumeration for explicitly supplied legacy references.
All enumerations load every reported candidate completely or return an error;
none return partial candidate sets or interpret missing data as an empty set.

## Identity contract

Equality compares concrete category and all components below recursively. It is
an equivalence relation over loaded values, not a PostgreSQL cast, coercion,
assignment, or overload-compatibility decision. It also compares snapshot content:
two named definitions with different loaded components are different values.

| Category | Identity components | Component accessors |
| --- | --- | --- |
| `base::BaseType` | Resolved qualified name | `name()` |
| `array::ArrayType` | Loaded element type | `element_type()` |
| `EnumType` | Resolved qualified name and ordered labels | `name()`, `labels()` |
| `DomainType` | Resolved qualified name and loaded base type | `name()`, `base_type()` |
| `CompositeType` | Optional qualified name and ordered `Members` | `name()`, `members()` |
| `ImplicitCompositeType` | Ordered `Members`, with its own category identity | `members()` |
| `TableRowType` | Resolved qualified table name and ordered `Members` | `name()`, `members()` |
| `Any` | No components; unconstrained | None |
| `Never` | No components; no possible type | None |
| `Candidates` | Set-wise identity of complete candidate types | `candidates()` |
| `Member` | Optional logical name, loaded type, and `is_not_null: Option<bool>` | `name()`, `member_type()`, `is_not_null()` |
| `Members` | Ordered members, including duplicates and unnamed members | `members()`, `member(name)` |

Member lookup uses an exact logical name and returns the first matching named
member. Unknown NOT NULL information (`None`) differs from `Some(false)` and
`Some(true)`. An implicit record, explicit unnamed composite, and table row
remain distinct categories even when their members match. A domain over an array
is distinct from an array of that domain. A base name containing `[]` is still a
base name; arrays are represented by wrappers.

Ownership, declaration constraints, default expressions, and declaration syntax
are not part of this loaded identity. Their omission does not migrate or discard
the legacy catalog's declaration semantics: those remain owned by the existing declaration
model. In particular, domain identity here does not claim to represent CHECK or
default-expression equivalence.

## Conversion and loading

`compat::from_legacy_definition` and `compat::from_legacy_reference` perform
context-free structural conversion. A terminal `Builtin` definition establishes
its category and can be converted directly, as can an enum with its ordered
labels. Dynamic references recurse into their owned definitions. Composite and
implicit members can be converted this way only when their component references
can also be converted without a catalog.

A concrete reference supplies a name and array qualifier, not the referenced
category. It therefore requires `compat::load_reference(context, reference)`;
it is never silently manufactured into a base type. `load_definition` uses the
same explicit context for nested names. Domain bases are resolved and loaded
through that context. Concrete array qualifiers on domain and composite identity
references become outer array wrappers; dynamic identity references are rejected
with `UnsupportedIdentityReference`, avoiding unfinished legacy name access.
Table rows are hydrated from the selected relation's
columns, preserving the row's array qualifier. Unsupported dynamic table-reference
payloads and mismatched table/reference names are rejected rather than ignored.

For example, a consumer that already selected a context can opt in at its boundary:

```rust
use sql_fun_sqlast::{sem, types};

fn load_selected_type<C: sem::ParseContext>(
    context: &C,
    reference: &sem::TypeReference,
) -> Result<types::TypeReference, types::compat::ConversionError> {
    types::compat::load_reference(context, reference)
}
```

The selected context resolves names; direct category constructors expect callers
to supply already resolved names. The loaders return typed `ConversionError`s for
missing context, missing types or relations, incomplete domain bases, missing
member types, unsupported named categories or table-reference shapes, recursion,
and excessive nesting. Finite owned values cannot represent recursive catalog
graphs; conversion detects active-name cycles and bounds nesting to 128 levels.

Concrete range/multirange and other pseudo-type catalog loading remains limited
by the legacy catalog data. Named polymorphic construction instead uses the
catalog-independent signature APIs described below.
The compatibility loader explicitly rejects recognized special builtins (for
example `pg_catalog.int4range`, `record`, `anyelement`, and `void`) with
`UnsupportedNamedType`; its spelling-based guard is not an exhaustive PostgreSQL
category classifier. For example, the semantic builtin catalog retains names
without range subtype or multirange base components. Those cannot be reconstructed
as complete loaded values, even when a cast relationship exists. Do not fabricate
`BaseType` values for these categories. Extending catalog classification and
extracting those components require separate work. `RangeType` and
`MultirangeType` can already own explicitly supplied
components, including polymorphic constraints, without catalog loading. SQL
inference and overload selection remain deferred operations.

## `sem::TypeDefinition` capability audit

The following covers its current methods, including crate-private catalog
capabilities, so adoption does not silently lose an obligation.

| Legacy API | Loaded API or disposition |
| --- | --- |
| `is_builtin()` | Inspect `BaseType` or `ArrayType` as appropriate; the legacy storage variant is not a loaded category. |
| `is_relation_type()` | Inspect `TableRowType`. |
| `is_domain()` | Inspect `DomainType`. |
| `owner()`, `change_owner()` | Remain declaration/catalog operations; intentionally absent from loaded identity. |
| `is_array()` | Inspect `ArrayType`; inspect `DomainType::base_type()` separately when the consumer needs the underlying shape. No universal optional flag or domain flattening. |
| `table_ref()` | Load through `compat` with a context, or construct `TableRowType` from an explicit resolved name and already loaded members. |
| `type_reference()` | The loaded value itself is the owned reference. Use `base_type()` or `element_type()` for actual components; no synthetic legacy reference is exposed. |
| `full_name()` | Category `name()` accessors; composites return an optional name. Arrays and implicit composites have no universal name. |
| `builtin()` | `BaseType::new` for an established terminal category; `compat` for legacy conversion and catalog-dependent loading. |
| `compatible_with()` | Deferred SQL compatibility API. `same_type` and equality compare loaded identity only. |
| `is_composite_type()` | Inspect `CompositeType`; implicit records and table rows retain their own categories. |
| `get_composite_type_columns()` | `CompositeType::members()` exposes owned `Members`, not declaration columns. |
| `get_member_type()`, `member_type_from_catalog()` | After loading a composite, implicit composite, or table row, use `members().member(name)?.member_type()` without another lookup. Non-record categories do not acquire member APIs. |
| `From<&CreateEnum>`, `From<&CreateCompositeType>`, `From<&CreateDomain>` | Explicit fallible `compat` conversion/loading; category constructors accept loaded components instead of declarations. |

The legacy `type_reference`, anonymous `full_name`, compatibility, and some member
lookup paths contain `todo!()` or category-specific unreachable cases. These are
not a specification to reproduce. Loaded access is category-specific and returns
optional member/name results where applicable. This seam does not implement the
missing SQL behavior in those legacy methods or change existing callers.

## `sem::TypeReference` capability audit

| Legacy API or capability | Loaded API or disposition |
| --- | --- |
| `dynamic()`, `type_definition()`, `is_dynamic()` | All loaded references own their definitions. Construct a category and box it, or use `compat`; the concrete/dynamic storage distinction disappears after loading. |
| `from_full_name()`, `concrete_type_ref()` | Resolve/load the selected reference with `compat`; explicit constructors require known category components. Array wrapping uses `ArrayType::new`. |
| `from_master_type_name()` | Legacy naming convention remains at ingestion; there is no name-prefix inference in loaded constructors. |
| `full_name()` | Use category-specific `name()` accessors; anonymous and structural values need no fabricated name. |
| `is_array()`, `element_type()` | Inspect `ArrayType` and access `element_type()`. Other categories do not expose an element. |
| `is_record()` | Inspect the relevant explicit composite, implicit composite, or table-row category; dynamic ownership does not itself imply record shape. Loading the pseudo-type `record` is deferred. |
| `is_range()`, `range_subtype()`, `is_multi_range()`, `multi_range_base_element()` | `RangeType::subtype()`, `MultirangeType::range().subtype()`; no suffix-based reconstruction. |
| `into_range_type()`, `into_multi_range_type()` | Construct `RangeType` / `MultirangeType` with explicit nominal identity and components; no subtype-only reconstruction. |
| `is_simple_infer_arg()`, `is_common_infer_arg()` | `PolymorphicCategory::family()`; inference execution remains deferred. |
| `is_any_element()`, `is_any_array()`, `is_any_non_array()`, `is_any_range()`, `is_any_multi_range()` | Simple-family `PolymorphicCategory` values and signature variables. |
| `is_any_compatible()`, `is_any_compatible_array()`, `is_any_compatible_non_array()`, `is_any_compatible_range()`, `is_any_compatible_multi_range()` | Common-family `PolymorphicCategory` values and independent signature variables. |
| `apply_simple_infer_result()`, `apply_common_infer_result()` | Deferred inference operations; loaded equality does not infer or mutate a result type. |
| `Display`, `FromStr` | Remain legacy concrete-reference text interfaces; no universal loaded textual encoding or parser is introduced. |
| `Clone`, `Eq`, `PartialEq`, `Debug` | Supported through owned components and the loaded trait's identity/clone contract. |
| `Hash`, serde serialization/deserialization | No loaded equivalents in this seam. Existing semantic serialization remains intact. |

Legacy dynamic branches with unfinished name, element, range, inference, or
formatting behavior are not copied into the loaded API. Consumers requiring a
deferred capability must remain on the existing boundary until that capability
has an explicit category and operation contract. This PR supplies the owned
representation and conversion seam, rather than changing existing semantic
analysis, overload selection, or storage to use it.

## Shared constraint categories

`Any::new()` explicitly constructs an unconstrained type, including the initial
state of an unnarrowed parameter. `Never::new()` constructs a valid constraint
state with no possible type. Neither category has components, and they remain
distinct from each other and from SQL pseudo/polymorphic types such as `unknown`
and `anyelement`.

`Candidates::from_candidates(&values)` owns complete clones in a private
`Vec<Box<dyn TypeDefinition>>`. A successful empty collection becomes `Never`;
a nonempty collection becomes `Candidates`, including a singleton. Candidate
identity is set-wise: order and duplicate count do not affect equality. Storage
preserves the supplied values and order, including nested constraints; there is
no flattening, ranking, automatic singleton selection, or inference. Equality
compares candidate values recursively. Thus a nested candidate category is not
identical to its flattened alternatives. These shared types can also hold whole
operator/function signatures for subsequent work; they are not cast-specific
containers.

## Loaded cast relationships

`types::CastDefinition` owns the ordered source and target loaded types and a
`types::CastContext`. Relationship equality compares all three components,
recursively comparing endpoint identities. It does not implement
`TypeDefinition`. `NoConversion` records the catalog's identity relationship;
neither it nor any other cast establishes type equality. The contexts retain
implicit preferred, implicit, assignment, explicit, and no-conversion distinctions.
No selection score or overload ranking is introduced.

Legacy `sem::CastDefinition` stores only context. Its endpoint references are
supplied separately; builtin JSON function/method fields are not available in
that semantic definition. `compat::load_cast_definition` adapts such a supplied
definition and endpoints through an explicitly selected `ParseContext`, preserving
its context and loading complete endpoints. It does not verify eligibility of
arbitrarily paired metadata.

`compat::load_implicit_cast`, `load_assignment_cast`, and `load_explicit_cast`
resolve concrete endpoint names through that same context, retaining array
qualification, then query the corresponding legacy lookup on those resolved
references. This name-resolving API intentionally compares with legacy calls on
resolved references, rather than raw unqualified inputs. Dynamic inputs retain
the legacy lookup's handling. Both endpoints must load completely before an
absence can be reported. `Ok(None)` is an absent direct relationship between
valid loaded endpoints; `Err(ConversionError)` is unavailable/incomplete endpoint
data. The returned context is the catalog's value, not the requested mode.
Assignment lookup delegates to the dedicated legacy assignment API to retain its
fallback behavior. Existing analyzers and call sites retain their interfaces.

`compat::load_cast_sources(context, target, explicit)` loads exactly the source
candidates from the selected legacy enumeration, with the caller choosing
implicit (`false`) or explicit (`true`) eligibility. `load_assignment_sources`
uses the corresponding assignment enumeration. `load_implicit_castable` instead
loads implicit target candidates from a supplied source. Completeness is the
existing catalog API's enumeration contract; none adds transitive cast searches.
The target is resolved and validated separately: candidate input types are not
the expression's target/result type. An empty successful enumeration becomes
`Never`; an error loading any candidate returns an error without a partial set.
Identity remains a candidate where the catalog enumerates it. Missing metadata
never becomes `Any` or `Never`. Constraint solving and semantic-consumer migration
remain outside this boundary.

### PostgreSQL coercion policy in the permanent type system

When the selected provider returns the PostgreSQL dialect, `TypeContext` delegates
these rules to `types/cast/resolve.rs`. `types/context.rs` retains the public
entrypoint, provider contract and delegation. Existing `sem::ParseContext` and
`sem::CastInfoRead` APIs, cast implementations and semantic consumers retain their
previous behavior. `types::compat` only adapts catalog facts and references.

| Behavior | Supported contract |
| --- | --- |
| Equal concrete loaded values | Intrinsic `NoConversion`, without catalog registration. Constraints and pseudo-types are excluded from this SQL rule. |
| Array to array | Recurse over `ArrayType::element_type()` in the same requested mode and direction. Preserve complete endpoints, including nested arrays and domain elements. A rejected element conversion remains rejected. |
| Domain to/from base or another domain | Recurse through loaded base components for eligibility, then retain the original domain endpoints. Different domain identities remain unequal; base-equivalent conversion is `Implicit`, not `NoConversion`. Domains over arrays use the same rules. |
| String I/O | With no overriding native registered cast, conversion to builtin string types is assignment-eligible; conversion from them is explicit-only. Scalar, array, enum, composite and domain endpoints are supported. Scalar/array element compatibility alone never establishes scalar-to-array conversion; string I/O is a separate rule. |
| Registered cast | `TypeContextRead::registered_cast` is authoritative for all modes, including explicit-only entries that block automatic coercion. A provider's `existing_array_cast` also precedes element derivation. |
| Candidate enumeration | Combine provider facts with arrays of reported element candidates, then filter through the permanent cast rules. Domain base candidates are likewise considered. This enumerates the selected provider's candidate universe, not every constructible anonymous type or domain. |

Builtin string classification covers `pg_catalog.text`, `varchar`, `bpchar` and
`name`. Custom string-category metadata is not represented in the loaded model;
a native provider must expose those relationships itself. Polymorphic
cast questions are explicitly deferred as described below; binding and inference
execution remain outside this API; `unknown` is not an ordinary
concrete cast endpoint. No transitive cast search is introduced.

A returned domain endpoint retains its nominal identity and loaded base. Domain
CHECK/default declarations remain in the existing declaration catalog; this API
does not execute casts, validate particular values, or compare CHECK expressions.
A successful eligibility lookup does not promise that a runtime value satisfies
the target domain's constraints.

Record-expression coercion is a separate expression-level operation: PostgreSQL
examines and converts individual expression fields in `coerce_record_to_complex`.
A pair of loaded type values contains no record expression, so this API does not
claim to implement that conversion. It does support concrete composite/string
I/O and provider-supplied composite cast relationships. Absence here is not a
verdict about whether a particular `ROW(...)` expression can be coerced.

#### Legacy provider limits

The adapter retains the selected legacy catalog's existing lookup and enumeration
facts. Its existing whole-array lookup can already include builtin derivation;
it is not a raw `pg_cast` row interface. The legacy API does not expose registered
row provenance separately from automatic I/O or derived relationships, so the
adapter cannot implement the native `registered_cast` fact hook. Native providers
can distinguish these facts and enforce exact registered-row precedence. Legacy
layer overrides whose meaning depends on that unavailable provenance retain this
interoperability limitation; no new legacy API or semantic behavior is introduced
to reconstruct it. For a complete input snapshot without a legacy
name/reference, the provider
returns no input catalog fact. This differs from failing to load a candidate
reported by the catalog: reported candidates still load completely or return
the existing loading error, never a partial set. Existing conversion/loading
errors remain practical adapter failures; no new cast-decision error category is
added.

These rules follow PostgreSQL's
[`CREATE CAST`](https://www.postgresql.org/docs/18/sql-createcast.html),
[type system](https://www.postgresql.org/docs/18/extend-type-system.html) and
[`find_coercion_pathway`](https://github.com/postgres/postgres/blob/REL_18_STABLE/src/backend/parser/parse_coerce.c).
Small rolled-back PostgreSQL 15–18 oracle queries verified domain and I/O
eligibility. Permanent regression tests use native providers and offline catalogs.

## PostgreSQL signature polymorphism

This section is for maintainers migrating semantic consumers and composing loaded
function signatures (#636), not for CLI users. No contributor steps change.

`SignaturePolymorphism::new(&positions)` atomically constructs all named
polymorphic occurrences of one signature, preserving their declaration order and
input/output/INOUT/variadic roles. Other concrete parameters remain owned by the
function-signature consumer. `PolymorphicPosition::new(category, role)` supplies
one declaration. `types()` returns immutable `PolymorphicType` values that can be
cloned/boxed into argument and result components without a catalog or
`ParseContext`. This scope is signature-local data, not an analyzer environment;
existing analyzers must still receive `ParseContext`.

`PolymorphicCategory` covers the six simple-family names (`anyelement`,
`anyarray`, `anynonarray`, `anyenum`, `anyrange`, `anymultirange`) and five common
names (`anycompatible`, `anycompatiblearray`, `anycompatiblenonarray`,
`anycompatiblerange`, `anycompatiblemultirange`). There is no `anycompatibleenum`.
The PostgreSQL special pseudo-type `any`, `unknown`, and unconstrained `types::Any`
are not named linked variables. PostgreSQL 15–18 expose the same eleven names.
The category matrix follows the PostgreSQL
[type system documentation](https://www.postgresql.org/docs/18/extend-type-system.html#EXTEND-TYPES-POLYMORPHIC).

Interpretation is explicit: `from_postgres_identifier` accepts a logical,
already-normalized/unquoted identifier, not raw SQL; a quoted lowercase
`"anyelement"` becomes the logical `anyelement`, while quoted `"AnyElement"`
remains different. `from_postgres_name` accepts only resolved `pg_catalog` names
without a catalog qualifier. It does not resolve unqualified/user-schema names.
The removable `compat::polymorphic_position` only adapts scalar legacy references
into position declarations. Collect these declarations and finalize **once per
signature**, not once per reference. The general catalog loader continues to
reject pseudo-type loading and does not silently construct independent scopes.

Each type's `variable()` provides a signature-scoped identity and related
`element()`, `array()`, `range()`, and `multirange()` variables. The two families
are independent. Enum/nonarray occurrences constrain the shared element variable,
including restrictions declared on outputs. Array and variadic occurrences keep
an element relationship without adding an unconditional `anynonarray`
restriction: PostgreSQL permits domains over arrays as array elements and in
variadic calls. The declaration role remains available through
`PolymorphicRole::is_variadic()` for future binding rules.
`requires_enum()` / `requires_nonarray()` expose
these requirements without executing them. Simple-family exact matching and
common-family common-type selection remain distinct. Common-family range and
multirange variables retain shared range identity and subtype agreement; subtype
coercibility alone cannot establish range compatibility.

Clones preserve binding identity and complete restrictions. Independent scopes
have distinct identity even with identical declarations. Equality includes the
category; `anyelement` and `anyenum` can share a variable without being identical
category values. This is binding identity, not alpha-equivalence of whole
function signatures; #636 must preserve scopes when assembling/cloning a
signature, and must not use freshly rebuilt scopes to compare binding identity.
No scope can be forged or extended after finalization.

Outputs require inputs in the same family. Range/multirange outputs require a
range/multirange input supplying the nominal identity; both cross-directions are
valid. Array/element output relations need no catalog to be represented. No
actual call type is required, so valid unresolved constraints are not rejected.
Variadic polymorphic positions require the applicable array category. Errors
refer to the index in the supplied polymorphic-position list.

`RangeType::new(name, subtype)` and `MultirangeType::new(name, range)` own typed
components, possibly constraints. Two ranges sharing a subtype can still have
different nominal identities. A multirange owns the complete related range.
There is no subtype-based choice of range identity. Their catalog ingestion is
still missing: the current legacy builtin records lack the complete range
subtype, multirange-to-range link, and reliable category facts needed for faithful
loading. Those facts must be supplied by future catalog extraction/adaptation,
coordinated with #634 cast endpoints and #636 concrete signature components;
this issue does not change the legacy storage model or fabricate those facts.
Named polymorphic signature constraints themselves have no such prerequisite.

| Existing inference concept | Representation / migration boundary |
| --- | --- |
| `is_simple_infer_arg`, `SimpleInferContext::collect` / `push_input_param` | Simple-family variable links and input roles; exact binding is future work. |
| `is_common_infer_arg`, `CommonInferContext::collect` / `push_input_param` | Independent common-family links; cast-based common-type selection remains future work. |
| Element/subtype/multirange-base extraction | Variable `element()` relation; concrete `ArrayType::element_type`, `RangeType::subtype`, `MultirangeType::range`. |
| `get_inference_result` and common candidate ranking | Future solver; no execution or candidate selection here. |
| `apply_simple_infer_result`, `apply_common_infer_result`, OUT/table results | Shared output occurrences identify the future substitution targets; legacy types are unchanged. |
| Variadic bindings | Variadic role, array variable, and shared element relationship; argument binding is not executed. |

Legacy simple inference omits `anyenum` dispatch, and legacy range reconstruction
contains `todo!()` paths. The new representation covers these categories without
changing those legacy algorithms, extending serialization, or activating a new
analyzer.

## Cast questions involving polymorphic constraints

All six public `TypeContext` cast methods now return `CastQueryOutcome<T>` inside
`Result<_, CastQueryError<C::Error>>`. Pair lookup uses
`T = Option<CastDefinition>`; enumeration uses `T = TypeReference`.
`resolution()` / `into_resolution()` inspect a determinate answer;
`deferred_query()` inspects a binding-dependent question. Callers must handle both
outcomes explicitly. A deferred enumeration is not a type candidate and must not
be converted into `Never`, `Any`, or a partial complete candidate set.

`DeferredCastQuery` owns source/target values as applicable, direction, mode and
selected dialect; cloning preserves all variable linkage. It executes no binding
or automatic retry. `CastQueryError::provider_error()` retains actual provider
failures; `unsupported_query_dialect()` identifies a PostgreSQL-constraint query
under SQLite. These errors do not invalidate constructed type values.

| Endpoints or query | Outcome |
| --- | --- |
| Recursively concrete pair | Resolved cast or known absence under existing provider/dialect rules. |
| Concrete ↔ polymorphic | Deferred; no implicit binding, catalog projection, or cast invented. |
| Polymorphic ↔ polymorphic, same or different scope/family | Deferred; variable identity alone is not a SQL identity cast. |
| Array/range/multirange containing constraints | Deferred whole-value query, retaining all structural/nominal relationships. |
| Other nesting (domain/composite members/candidate alternatives) | Recursively deferred before any provider facts are requested. |
| Mixed scalar/container shapes containing constraints | Deferred, since shape mismatch alone cannot exclude a registered whole-type cast. |
| Enumeration on a constrained value | Deferred enumeration, retaining its direction and mode. |
| PostgreSQL constraint under SQLite | Explicit unsupported-dialect operation error, including recursive nesting. |
| Concrete SQLite operation | Existing provider behavior wrapped in a resolved outcome. |

`TypeDefinition::contains_polymorphic()` is the permanent recursive capability
used before provider dispatch. Structural implementations must inspect owned
components; the default describes a leaf without these constraints. Existing
builtins recurse through arrays, domains, all record members, ranges,
multiranges, and candidates. No legacy projection failure determines the answer
to a polymorphic query. `TypeContextRead` remains the concrete-fact provider
contract, with its existing return types; native providers need not implement
polymorphic inference. `compat` remains catalog adaptation only.

For fully concrete ranges, intrinsic equality and existing provider casts can
produce determinate answers. No cast is derived merely from range subtype casts.
Determining more constrained cases would require binding/solving; this issue
conservatively preserves those questions rather than adding such an algorithm.
The legacy `sem` cast APIs and all existing analyzer consumers remain unchanged.
