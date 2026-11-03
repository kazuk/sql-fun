# CREATE TABLE / CREATE TABLE AS

## Overview

CREATE TABLE analysis retains ordered column declarations and analyzed
attributes, types, defaults, nullability, generated/identity settings and
supported constraints. Applicable definitions enter the analysis catalog for
later statements; no table is created in a database. [Shared creation
conflicts](../statements.md) explain namespaces and IF NOT EXISTS behavior.

```sql
CREATE TABLE public.items (id integer PRIMARY KEY, label text);
```

PostgreSQL primary-key columns receive non-null metadata. Named and unnamed
table-level foreign keys, including composite keys, retain source columns,
selected referenced relation and target columns. Omitting referenced columns
uses the referenced primary key when available. Failed lookups retain their
outcome; a rejected declaration is not published merely because an analyzed
statement was produced. Original declaration type modifiers and explicit
declaration collations are retained separately from canonical type identity; see
[types and casts](../expressions/types-and-casts.md) and
[COLLATE](../expressions/collate.md).

### CREATE TABLE AS

The PostgreSQL table-as path analyzes its query and derives ordered columns. A
supplied column-name list renames outputs by position; too many names or
duplicate outputs are diagnosed. Ordinary CREATE TABLE AS publishes a table
definition. The materialized-view variant retains the analyzed query as a
materialized view. Neither operation executes the query or populates rows; a
WITH NO DATA spelling does not turn static analysis into data loading.

## Dialect and engine-specific behavior

SQLite CREATE TABLE has a dedicated conversion and analysis path behind the
sqlite feature, including declared types and retained foreign-key declarations.
It does not use PostgreSQL assignment or implicit-cast rules. SQLite permits
repeated constraint names, but that remains unsupported by the retained schema
model. Automatically generated database index names are not inferred. SQLite
CREATE TABLE AS currently reaches an unimplemented table-query analysis path;
PostgreSQL table-as support does not establish SQLite support.

Inheritance/partition analysis remains incomplete
([#546](https://github.com/kazuk/sql-fun/issues/546)), and deferrable unique-key
catalog definitions are unimplemented. PostgreSQL 18 VIRTUAL declarations are
recognized and version-gated. Temporal and new constraint
enforcement/inheritance forms have retained syntax and explicit unsupported
paths; grammar acceptance is not full constraint validation.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): a table,
  relation identity or retained constraint name conflicts with an existing
  declaration.
- [column_type_not_found](../../diagnostics/column_type_not_found.md): a
  declared column type cannot be resolved.
- [relation_not_found](../../diagnostics/relation_not_found.md): a referenced
  relation cannot be resolved.
- [column_not_found_in_table](../../diagnostics/column_not_found_in_table.md): a
  retained foreign-key source or target column cannot be resolved.
- [foreign_key_referenced_primary_key_missing](../../diagnostics/foreign_key_referenced_primary_key_missing.md):
  an omitted foreign-key target list requires a primary key that is not
  available.
- [incompatible_engine_version](../../diagnostics/incompatible_engine_version.md):
  an implemented version gate rejects a PostgreSQL 18 feature for the explicitly
  selected version.

Declaration collation and expression diagnostics are described on their shared
pages.
