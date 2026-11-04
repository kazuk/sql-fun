# CREATE TABLE / CREATE TABLE AS

## Overview

CREATE TABLE analysis checks column declarations in written order and analyzes
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
uses the referenced primary key when available. Missing referenced objects
produce diagnostics; rejected declarations do not become available to later
schema analysis. Original declaration type modifiers and explicit
declaration collations are checked separately from the column type; see
[types and casts](../expressions/types-and-casts.md) and
[COLLATE](../expressions/collate.md).

### PostgreSQL serial columns

Schema loading accepts `smallserial`/`serial2`, `serial`/`serial4`, and
`bigserial`/`serial8` in CREATE TABLE column declarations. Later queries see
`smallint`, `integer`, and `bigint`, respectively. Serial columns are implicitly
NOT NULL and have a sequence-backed default; serial alone does not declare a
primary key or uniqueness. Written key constraints still apply.

The generated sequence is registered in the analysis catalog with a dependency
on its table. Analysis does not execute `nextval` or allocate database values.
Column-level sequence ownership and complete sequence-option validation remain
unsupported ([#545](https://github.com/kazuk/sql-fun/issues/545)); see
[sequences](sequences.md). Serial is declaration shorthand, so it is not added
as a general cast or function-signature type. A quoted lowercase `"serial"`
declaration has the same meaning; qualified names and differently cased quoted
names use ordinary type lookup. Invalid serial declarations report specific diagnostics:

- Arrays: [serial_array](../../diagnostics/serial_array.md).
- Type modifiers: [serial_modifier](../../diagnostics/serial_modifier.md).
- Explicit NULL: [serial_nullable](../../diagnostics/serial_nullable.md).
- Explicit DEFAULT: [serial_default_conflict](../../diagnostics/serial_default_conflict.md).
- IDENTITY: [serial_identity_conflict](../../diagnostics/serial_identity_conflict.md).
- Generated expressions: [serial_generated_conflict](../../diagnostics/serial_generated_conflict.md).

### CHECK constraints

Named and unnamed table and inline column CHECK constraints are analyzed against
all declared columns, including columns written after the constraint. Inline
CHECK remains attached to its declared column and may reference other columns.
PostgreSQL requires a boolean expression; NULL and valid unknown boolean strings
are accepted. A CHECK permits NULL results and does not imply NOT NULL column
metadata. Subqueries,
aggregates, window functions, set-returning expressions and bind parameters
are rejected. Failed definitions do not enter the analysis catalog.

```sql
CREATE TABLE bounds (
    low integer CONSTRAINT ordered CHECK (low < high) NO INHERIT,
    high integer
);
```

Constraint names, NO INHERIT and applicable explicit enforcement choices are
retained. CREATE table-level CHECK accepts NOT VALID but records it as validated;
ALTER ADD CHECK NOT VALID instead retains an unvalidated constraint.
CHECK is not deferrable. Inheritance propagation remains
[#546](https://github.com/kazuk/sql-fun/issues/546);
PostgreSQL 18 NOT ENFORCED remains
[#887](https://github.com/kazuk/sql-fun/issues/887). SQLite table and inline CHECK
constraints use SQLite truth values rather than PostgreSQL boolean coercion.
SQLite CHECK constraints using ordinary comparison/arithmetic operators can still
reach the shared unimplemented
operator path ([#504](https://github.com/kazuk/sql-fun/issues/504)); supported
truth/null tests and logical expressions can bind multiple row columns.
Analysis does not execute
checks against database rows or infer an implicit index. ALTER ADD COLUMN,
including columns with inline CHECK, remains unimplemented
([#528](https://github.com/kazuk/sql-fun/issues/528)); CREATE support does not
establish ALTER support.

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
repeated constraint names, but that remains unsupported by schema analysis;
repeated named table or inline CHECK constraints reach an explicit unimplemented
path
([#992](https://github.com/kazuk/sql-fun/issues/992)).
Automatically generated database index names are not inferred. SQLite CREATE
TABLE AS currently reaches an unimplemented table-query analysis path;
PostgreSQL table-as support does not establish SQLite support.

Inheritance/partition analysis remains incomplete
([#546](https://github.com/kazuk/sql-fun/issues/546)), and deferrable unique-key
catalog definitions are unimplemented. PostgreSQL 18 VIRTUAL declarations are
recognized and version-gated. Temporal and new constraint
enforcement/inheritance forms have retained syntax and explicit unsupported
paths; grammar acceptance is not full constraint validation.

### Remaining declaration limitations

PostgreSQL EXCLUDE constraint analysis remains unimplemented
([#526](https://github.com/kazuk/sql-fun/issues/526)) and can terminate analysis.

`CREATE TABLE name OF composite_type` is recognized but does not resolve
and apply the typed table's column structure and dependent type association
([#657](https://github.com/kazuk/sql-fun/issues/657)). Inheritance, partition
bounds/keys and relation options such as ON COMMIT, access method and storage
settings also lack complete semantic application
([#546](https://github.com/kazuk/sql-fun/issues/546)). Declared columns alone do
not model inherited/typed-table columns or partition relationships.

Constraint modifier analysis is incomplete
([#563](https://github.com/kazuk/sql-fun/issues/563)): initial deferral,
validation/NO INHERIT flags, NULLS NOT DISTINCT, included columns and constraint
index/storage options are not all retained and validated as analyzed constraint
properties. Deferrable unique keys remain explicitly unimplemented. Retained
foreign-key delete-column lists are supported metadata; that does not complete
the other modifiers. Column declaration collations are analyzed as described
above, but column compression and storage attributes are not yet applied by
CREATE TABLE analysis
([#564](https://github.com/kazuk/sql-fun/issues/564)).

PostgreSQL 18 temporal keys (`WITHOUT OVERLAPS`), foreign-key PERIOD columns,
NOT ENFORCED checks, table-level NOT NULL, inline NOT NULL NO INHERIT and new
NOT NULL validation attributes do not have complete constraint semantics
([#887](https://github.com/kazuk/sql-fun/issues/887)). Retention/version gating
can precede an unimplemented path; accepting PostgreSQL 18 grammar does not
establish successful catalog application.

## Diagnostics

- [check_boolean_type](../../diagnostics/check_boolean_type.md): a CHECK boolean
  operand has an incompatible PostgreSQL type.
- [check_boolean_literal](../../diagnostics/check_boolean_literal.md): a CHECK
  uses invalid PostgreSQL boolean literal input.
- [check_expression_placement](../../diagnostics/check_expression_placement.md):
  a CHECK contains a prohibited expression form.
- [check_parameter](../../diagnostics/check_parameter.md): a CHECK uses a bind
  parameter.
- [check_deferrability](../../diagnostics/check_deferrability.md): a CHECK
  requests deferral.
- [check_duplicate_name](../../diagnostics/check_duplicate_name.md): a CHECK
  repeats an explicit PostgreSQL constraint name.
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
