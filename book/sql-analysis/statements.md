# Top-level statements

Choose the statement you are writing or loading from a schema file. These pages
describe analysis support, not a promise that every subform accepted by the
SQL parser can be analyzed or executed.

- Queries: [SELECT](statements/select.md).
- Changes to rows: [INSERT](statements/insert.md), [UPDATE](statements/update.md),
  [DELETE](statements/delete.md), and [MERGE](statements/merge.md).
- Tables: [CREATE TABLE and CREATE TABLE AS](statements/create-table.md),
  [ALTER TABLE](statements/alter-table.md), and [DROP TABLE](statements/drop-table.md).
- Views: [CREATE VIEW](statements/create-view.md) and
  [ALTER VIEW](statements/alter-view.md).
- Indexes: [CREATE INDEX](statements/create-index.md) and
  [ALTER INDEX](statements/alter-index.md).
- Write behavior: [CREATE/DROP RULE](statements/rules.md) and
  [CREATE/DROP TRIGGER](statements/triggers.md).
- Types and routines: [CREATE TYPE](statements/create-type.md),
  [CREATE DOMAIN](statements/create-domain.md), and
  [CREATE FUNCTION](statements/create-function.md).
- Other schema statements: [sequences](statements/sequences.md),
  [CREATE SCHEMA](statements/create-schema.md),
  [ALTER ... OWNER TO](statements/alter-owner.md),
  [CREATE EXTENSION](statements/create-extension.md), [SET](statements/set.md),
  and [COMMENT](statements/comment.md).

## Conflicting schema definitions

Schema analysis diagnoses duplicate definitions in supplied schema files,
including conflicts with earlier files. A rejected declaration preserves the
existing definition, and analysis continues with later statements.
Tables, views, sequences, and indexes share relation names in PostgreSQL;
standalone composite types also occupy that namespace. Other types and routine
overloads follow their own namespace rules. Names in different schemas remain
distinct.

Supported `IF NOT EXISTS` forms preserve existing definitions. PostgreSQL `OR
REPLACE` permits compatible view and routine replacements; it does not permit
replacing an unrelated object kind. SQLite table/view names are compared without
ASCII case distinctions, including quoted names. SQLite `CREATE INDEX` semantic
analysis and automatically generated database index names remain unsupported.
SQLite tables with repeated constraint names also remain unsupported by schema
analysis, although SQLite itself permits them.

[DROP TABLE](statements/drop-table.md) is retained as schema metadata; it does
not remove the definition or free its name for another CREATE. Supply the
intended schema definitions rather than treating schema files as a migration
history.

The [conflicting_creation](../diagnostics/conflicting_creation.md) diagnostic
reports an object-identity conflict. Statement-specific existence diagnostics
and permitted no-op or replacement behavior are described on the statement
pages.

## Unimplemented SQLite and Turso statement analysis

The following SQLite/Turso statements are recognized but cannot yet be analyzed
or used to change the schema known to subsequent analysis. Using them can
terminate analysis instead of producing a structured diagnostic.

| Statement family | Missing analysis | Tracking issue |
| --- | --- | --- |
| Maintenance | ANALYZE and VACUUM | [#549](https://github.com/kazuk/sql-fun/issues/549) |
| Attached databases | ATTACH and DETACH | [#550](https://github.com/kazuk/sql-fun/issues/550) |
| Transactions | BEGIN, COMMIT, ROLLBACK, SAVEPOINT and RELEASE | [#551](https://github.com/kazuk/sql-fun/issues/551) |
| Index lifecycle | CREATE INDEX and DROP INDEX | [#552](https://github.com/kazuk/sql-fun/issues/552) |
| Index maintenance | REINDEX and OPTIMIZE | [#553](https://github.com/kazuk/sql-fun/issues/553) |
| Virtual tables | CREATE VIRTUAL TABLE | [#554](https://github.com/kazuk/sql-fun/issues/554) |
| View removal | DROP VIEW | [#556](https://github.com/kazuk/sql-fun/issues/556) |
| Connection settings | PRAGMA statements | [#557](https://github.com/kazuk/sql-fun/issues/557) |
| Materialized views | CREATE MATERIALIZED VIEW | [#558](https://github.com/kazuk/sql-fun/issues/558) |
| Named types | CREATE TYPE and DROP TYPE | [#559](https://github.com/kazuk/sql-fun/issues/559) |
| Sequences | CREATE SEQUENCE and DROP SEQUENCE | [#560](https://github.com/kazuk/sql-fun/issues/560) |
| Domains | CREATE DOMAIN and DROP DOMAIN | [#561](https://github.com/kazuk/sql-fun/issues/561) |

This inventory includes recognized Turso
extensions; it does not assert that every form is legal in stock SQLite.
PostgreSQL support for a similarly named statement does not implement these
SQLite/Turso operations. In particular, an explicit PRAGMA cannot currently
establish connection state for later analysis. Rename propagation has the
additional [PRAGMA-state limitation](statements/alter-table.md#rename-table).

## Other PostgreSQL utility statements

Analysis remains unimplemented
for `ALTER DEFAULT PRIVILEGES ... ON LARGE OBJECTS` and
`CREATE FOREIGN TABLE (... LIKE ...)`
([#891](https://github.com/kazuk/sql-fun/issues/891)). These valid PostgreSQL
forms can reach an unimplemented analysis failure before semantic analysis;
parser recognition and engine-version checks do not supply their missing
privilege or foreign-table semantics.
