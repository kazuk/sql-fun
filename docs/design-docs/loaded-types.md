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
| `Member` | Optional logical name, loaded type, and `is_not_null: Option<bool>` | `name()`, `member_type()`, `is_not_null()` |
| `Members` | Ordered members, including duplicates and unnamed members | `members()`, `member(name)` |

Member lookup uses an exact logical name and returns the first matching named
member. Unknown NOT NULL information (`None`) differs from `Some(false)` and
`Some(true)`. An implicit record, explicit unnamed composite, and table row
remain distinct categories even when their members match. A domain over an array
is distinct from an array of that domain. A base name containing `[]` is still a
base name; arrays are represented by wrappers.

Ownership, constraints, default expressions, and declaration syntax are not part
of this loaded identity. Their omission does not migrate or discard the legacy
catalog's declaration semantics: those remain owned by the existing declaration
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

Range, multirange, polymorphic, and pseudo-type behavior is outside this seam.
The compatibility loader explicitly rejects recognized special builtins (for
example `pg_catalog.int4range`, `record`, `anyelement`, and `void`) with
`UnsupportedNamedType`; its spelling-based guard is not an exhaustive PostgreSQL
category classifier. Extending catalog classification and representing these
categories require separate work. SQL inference and cast compatibility also
remain separate, deferred operations.

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
| `is_range()`, `range_subtype()`, `is_multi_range()`, `multi_range_base_element()` | Deferred range categories and component APIs; no suffix-based reconstruction. |
| `into_range_type()`, `into_multi_range_type()` | Deferred category construction with actual range metadata. |
| `is_simple_infer_arg()`, `is_common_infer_arg()` | Deferred inference classification. |
| `is_any_element()`, `is_any_array()`, `is_any_non_array()`, `is_any_range()`, `is_any_multi_range()` | Deferred simple polymorphic categories. |
| `is_any_compatible()`, `is_any_compatible_array()`, `is_any_compatible_non_array()`, `is_any_compatible_range()`, `is_any_compatible_multi_range()` | Deferred compatible-family polymorphic categories. |
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
