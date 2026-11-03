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

Supported `IF NOT EXISTS` forms preserve existing definitions. PostgreSQL
`OR REPLACE` permits compatible view and routine replacements; it does not permit
replacing an unrelated object kind. SQLite table/view names are compared without
ASCII case distinctions, including quoted names. SQLite `CREATE INDEX` semantic
analysis and automatically generated database index names remain unsupported.
SQLite tables with repeated constraint names also remain unsupported by the
retained schema model, although SQLite itself permits them.

[DROP TABLE](statements/drop-table.md) is retained as schema metadata; it does not
remove the definition or free its name for another CREATE. Supply the intended
schema definitions rather than treating schema files as a migration history.

The [conflicting_creation](../diagnostics/conflicting_creation.md) diagnostic
reports an object-identity conflict. Statement-specific existence diagnostics
and permitted no-op or replacement behavior are described on the statement pages.
