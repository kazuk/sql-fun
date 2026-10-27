# Schema creation collision audit

This is the implementation audit for [#445](https://github.com/kazuk/sql-fun/issues/445).
It describes supported semantic application paths, not everything accepted by the
underlying parsers. Schema analysis retains definitions; it is not a migration
executor.

## Catalog design

`SchemaCatalog.objects: TrieMap<Kind>` indexes the kinds occupying shared relation
names. Definition bodies remain in their existing catalogs. Keys use canonical,
component-aware `FullName::Display` after dialect identity normalization. Exact
`ObjectInfoRead::object_kind` reads follow local definitions and frozen parents;
creation checks do not expand an explicit identity through unrelated schemas.
SQLite relation identities fold ASCII case, including quoted identifiers.

Definition write/update operations maintain kind entries themselves. There is no
kind-only write API. Ownership changes use those same definition writes. Types,
routines, schemas, extensions, constraints, and operators are not indiscriminately
inserted into the relation index. PostgreSQL standalone composite types are the
exception: their backing relation occupies the relation namespace as well.

All analyzers retain `ParseContext` as their common environment. Creation failures
use structured `CreationProblem` diagnostics and leave existing definitions intact.

## Paths and collision domains

| Object/form | Supported application path | Identity and policy |
| --- | --- | --- |
| PostgreSQL table | `ParseContext::apply_create_table` | Exact creation schema and relation name; also check row-type namespace. `IF NOT EXISTS` preserves any existing relation. |
| SQLite table | Same application path | Database and ASCII case-insensitive name. `IF NOT EXISTS` skips table/view names, not index names. |
| Ordinary view | `ViewInfoWrite::apply_create_view` | Shared relation name; PostgreSQL also publishes a row type. PostgreSQL `OR REPLACE` requires an ordinary view and preserves existing column names/types in order; additional columns are allowed. SQLite retains `IF NOT EXISTS`. |
| Materialized view | `ViewInfoWrite::apply_create_table_as` | PostgreSQL relation and row-type names. Preserve `IF NOT EXISTS`. Do not pre-register an ordinary table. |
| PostgreSQL CREATE TABLE AS | `TableAs::resolve_and_apply_at` | Table creation policy; avoid the unrelated unsupported low-level non-materialized view-writer branch. |
| Sequence | `SequenceInfoWrite::apply_create_sequence` | PostgreSQL shared relation name; preserve `IF NOT EXISTS`. |
| Explicit PostgreSQL index | `ParseContext::apply_create_index` | Target table's resolved schema, not the first search-path schema. Shared relation name; preserve `IF NOT EXISTS`. |
| Constraint-created index | Table CREATE/ALTER application | Preflight explicitly named key indexes before publishing the table/constraint or row type. SQLite constraint names are not relation/index names. |
| Composite type | `ParseContext::apply_create_comp_type`, `TypeInfoWrite` | PostgreSQL type name and backing relation name. |
| Enum/domain | `ParseContext::apply_create_enum` / `apply_create_domain` | PostgreSQL type namespace, including inherited row types. Legal same-name indexes/sequences are not rejected merely for sharing a type's name. |
| Function/procedure | `FunctionInfoWrite::apply_create_function_impl` | Existing routine catalog, exact name plus input argument types; FUNCTION/PROCEDURE cannot share the same signature. Legal overloads coexist with relations. `OR REPLACE` must preserve routine kind and return type; inherited overloads and ownership survive. |
| Schema | `ParseContext::apply_create_schema` | Database-wide schema namespace. Duplicate diagnostics or explicit `IF NOT EXISTS` no-op. Check before evaluating nested declarations. |
| Extension | `ParseContext::apply_create_extension` | Database-wide extension namespace across frozen layers; retain `IF NOT EXISTS`. This operation records the extension, without loading its definition file. |
| Constraints | Retained in table definitions | Table-local names; named primary/unique keys also occupy relation names through their PostgreSQL backing index. |
| Operators, casts, collation/opclass inventory | Catalog inventory/read services | Separate signature/provenance catalogs. No CREATE statement application added by this change. |
| Comments, SET, ALTER OWNER | Existing update paths | Intentional updates, not duplicate creation. Keep relation kind synchronized during definition updates. |

Previously table/type/schema/extension/index/sequence/view writers inserted without
creation-time conflict validation; routine writers replaced matching signatures.
The per-kind storage boundary alone did not express shared SQL namespaces.

## Layering and statement publication

CREATE TABLE checks the relation, row type, and named implicit indexes before
publishing definitions. ALTER TABLE mutates a candidate table and checks its
implicit indexes before publishing the candidate or dependent ownership changes.
Rejected operations keep diagnostics and allow later statements to be analyzed.
Frozen parents and siblings are never mutated.

CREATE SCHEMA checks duplicate schema names before nested application and stages
nested catalog writes in a sparse child layer. A rejected nested declaration
discards the group while retaining diagnostics.

`DROP TABLE` has an existing, explicit **record-only** contract: the table remains
readable and continues occupying its name. No tombstone or name reuse is introduced
for this operation. Relation/column renaming still reaches existing unimplemented
semantic application branches. There is consequently no supported effective
remove/move operation that requires a new tombstone representation in this change.
A future effective DROP/RENAME implementation must mask both definition and kind
reads so frozen ancestors do not reappear, and must update dependent identities.

## Unsupported or incomplete forms

- SQLite CREATE INDEX, CREATE TRIGGER, and ATTACH/DETACH do not have supported
  top-level semantic dispatch. Trigger names belong to a distinct namespace;
  do not put future triggers into the table/view/index collision domain.
- SQLite CREATE TABLE AS reaches an existing unimplemented table-query branch.
- SQLite permits repeated constraint names, but the existing retained table model
  keys constraints by name and cannot represent that form. It remains unsupported;
  the PostgreSQL duplicate-constraint guard is not applied to SQLite.
- Materialized-view creation retains output metadata, but direct column-reference
  services in `CreateTableAs` still have existing unimplemented branches.
- Foreign-table creation and several ALTER statement families are unsupported.
- Automatically generated database index/constraint names are not inferred by the
  existing schema model. This change validates explicitly named constraint indexes;
  it does not claim full PostgreSQL automatic-name generation or SQLite internal
  `sqlite_autoindex_*` modeling.
- Complete CREATE OR REPLACE validation beyond identity/column/return-type rules
  (for example routine argument-name and OUT-parameter compatibility) remains
  outside the existing DDL model.
- Schema existence, attached-database inventory, and PostgreSQL session-specific
  temporary-schema identity are not fully modeled. Explicit temporary declarations
  use logical `pg_temp` (PostgreSQL) or `temp` (SQLite), separate from permanent
  creation names. Unqualified relation/type lookup searches temporary schemas
  first unless
  explicitly positioned in the search path. PostgreSQL routine lookup excludes
  the temporary schema. Session-specific physical temporary-schema names remain
  outside this model.

These boundaries are limitations, not successful dispatch or coverage claims.

## Validation references

Small transactional PostgreSQL oracle probes on versions 15, 16, 17, and 18 verified
relation conflicts, table row-type conflicts, constraint-index conflicts, and
FUNCTION/PROCEDURE signature conflicts. PostgreSQL 18 additionally verified the
composite-type backing relation and legal enum/index coexistence. All probes
rolled back their changes. Ordinary tests do not require these servers.

Review follow-up probes on PostgreSQL 15 and 18 confirmed duplicate column CHECK
names are rejected, while named NOT NULL constraints enter the constraint-name
namespace only in PostgreSQL 18. Named DEFAULT clauses do not occupy it.
PostgreSQL 18 also confirmed explicit `pg_temp` search-path positioning and
rejection of an overlong CREATE TABLE AS column list.

SQLite regression cases compare accepted/rejected creation with an in-memory
`rusqlite` connection, using the existing dev dependency; no SQLite CLI is required.

- [PostgreSQL search path](https://www.postgresql.org/docs/18/runtime-config-client.html#GUC-SEARCH-PATH)
- [PostgreSQL CREATE TABLE AS](https://www.postgresql.org/docs/18/sql-createtableas.html)
- [PostgreSQL CREATE TABLE](https://www.postgresql.org/docs/18/sql-createtable.html)
- [PostgreSQL CREATE INDEX](https://www.postgresql.org/docs/18/sql-createindex.html)
- [PostgreSQL CREATE TYPE](https://www.postgresql.org/docs/18/sql-createtype.html)
- [PostgreSQL CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)
- [PostgreSQL CREATE VIEW](https://www.postgresql.org/docs/18/sql-createview.html)
- [SQLite CREATE TABLE](https://www.sqlite.org/lang_createtable.html)
- [SQLite CREATE VIEW](https://www.sqlite.org/lang_createview.html)
- [SQLite CREATE INDEX](https://www.sqlite.org/lang_createindex.html)
