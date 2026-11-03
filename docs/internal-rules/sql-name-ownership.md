# SQL name ownership

The crate-level `sql_fun_sqlast::names` module owns logical SQL identities.
This boundary is for implementers and API consumers updating name imports.
It is the first, isolated migration in #942; the broader AST/semantic ownership
inversion remains #926.

| Representation | Owner and purpose |
| --- | --- |
| `CatalogName`, `SchemaName`, `LocalName`, `FullName` | `names`: logical identifier components and qualification. |
| `OperatorName`, `ValidOperatorName` | `names`: operator identity and retained valid/invalid state. |
| `ColumnName`, `TableName`, `ViewName` | `names`: column and relation identity wrappers. |
| `AliasName`, `CteName` | `names`: source alias identity and the existing CTE string alias. |
| `AliasSpec` | `sem`: explicit alias provenance, derived columns and binding behavior. |
| `QueryName`, `QueryNameResolution`, `ResolvedOperator` | `sem`: completed resolution/binding outcomes. |
| `Role`, `OwnerSpecification` | `sem`: environment-supplied roles and completed owner bindings, including symbolic session owners. |
| `RuleIdentity`, `TriggerIdentity` | `sem`: resolved owning-relation and dialect namespace keys. |
| `OwnershipIdentity` | `sem`: catalog object kind, overload signature and access method. |
| `CollationIdentity`, `CatalogTypeIdentity`, `OperatorFamilyIdentity` | `sem`: catalog extraction records preserving source field schemas; collation/operator-family records also retain encoding/access-method properties. |
| `CatalogNameLookup` | `sem`: catalog lookup outcome, not a catalog identifier. |
| AST `Name` and `TypeName` | `ast`: syntax representations, including type syntax. |
| SQLite `Name` and `QualifiedName` | `syn::sqlite`: dialect syntax representations. |
| Generated PostgreSQL `TypeName` | `syn`: generated parser representation. |
| Client type parser's `ParsedName` | `sem::client_type`: private parser state. |

The old `sem` identity reexports are removed. There is one definition of each
identity; obsolete paths do not have compatibility aliases. The repository's
pre-SemVer policy permits this intentional API relocation. Package versions and
release intent are unchanged; API validation still uses the active baseline.
`AliasName`, previously available through an effectively internal path, now has
an explicit public identity path alongside the other names.

## Representation and interpretation

`names` retains logical constructors, comparison/hash behavior, canonical
formatting, serialized representation and name-only component parsing. Private
fields stay private. Existing name operations are reused by semantic adapters
rather than exposing representation fields for conversion.

`sem::name_conversion` owns parser-node conversions and adapters whose results
include `AnalysisError` or `ProblemLikeError`. Inherent and trait implementations
can live in a processing module independently of the type definition. In
particular, `FullName::from_str` keeps its existing `AnalysisError` associated
type and converts failure from the name-only component parser. Canonical parsing
preserves case and is distinct from SQL parsing.

`sem::name_analysis` retains builtin-type classification and inference helpers,
including existing unimplemented range operations. Semantic analyzers continue
to use `ParseContext` for environment access and diagnostic reporting.
`sem::parse_client_type` retains its public path, explicit dialect selection,
`TypeReference` result and existing client-protocol limitations.

`FullName::to_sql` continues to delegate to the existing AST SQL renderer with
an explicitly supplied dialect. The renderer consumes `names::FullName` and
preserves its dialect-specific validation. This migration does not make the
whole `ast` module independent of `sem`; that is separate work in #926.

No serialized field names or identity representations change. A catalog still
requires a schema. Canonical display still quotes components independently and
remains suitable for existing search keys. PostgreSQL SQL parsing performs its
existing folding, and SQLite identity operations keep their existing comparison
rules. No omitted choice gains a default or fallback.
