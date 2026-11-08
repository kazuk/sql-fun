# ALTER TABLE

## Overview

Supported schema changes update the analysis catalog for later statements in
explicitly selected schema files; they do not execute database migrations. See
the [shared schema-definition policy](../statements.md).

### Adding table CHECK constraints

`ADD [CONSTRAINT name] CHECK (condition)` retains the analyzed expression and
its target-column bindings. Multiple unnamed checks remain distinct. NO INHERIT
and NOT VALID are retained; NOT VALID describes omitted validation of existing
rows, not permission to accept an invalid expression. Analysis does not inspect
live rows. CHECK is not deferrable. PostgreSQL 18 NOT ENFORCED and constraint
state transitions remain [#887](https://github.com/kazuk/sql-fun/issues/887).

An invalid CHECK leaves the entire ALTER statement unapplied. Column and table
renames update retained CHECK bindings. See [table CHECK rules](create-table.md#check-constraints)
for expression restrictions and NULL behavior. SQLite has no ADD CONSTRAINT
counterpart.

### Column attributes

Schema analysis supports these operations on ordinary table columns:

- `SET NOT NULL` and `DROP NOT NULL`, including their effects on subsequent
  query result nullability. Dropping NOT NULL from a primary-key or identity
  column is rejected.
- `SET STATISTICS` with integer targets from `-1` through `10000`. `-1` restores
  the database default; higher targets are reduced to `10000` with a warning.
  The `DEFAULT` spelling requires an explicitly configured PostgreSQL version of
  17 or later.
- `SET` and `RESET` for `n_distinct` and `n_distinct_inherited`. RESET also
  accepts unknown or repeated option names as no-ops, as PostgreSQL does.
- `DROP EXPRESSION [IF EXISTS]` for stored generated columns. After removal,
  INSERT and UPDATE treat the column as an ordinary column. A virtual generated
  column cannot use this operation. PostgreSQL 18's `VIRTUAL` declaration syntax
  is recognized and version-gated; parsing it does not make DROP EXPRESSION
  applicable to virtual generated columns.

These operations can be combined in one ALTER TABLE statement. Analysis follows
PostgreSQL's operation phases: for example, combining SET NOT NULL and DROP NOT
NULL leaves the column NOT NULL regardless of their textual order. An invalid
operation leaves the catalog unchanged for the whole statement, and later
statements can still be analyzed. Analysis updates schema metadata; it does not
scan database rows to establish that existing values satisfy a new constraint.

Applying combinations with column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057). Combining these operations with
other ALTER commands, changing a generation
expression with `SET EXPRESSION`, changing storage/compression, generic foreign
column options, and applying column attributes through ALTER INDEX or ALTER
FOREIGN TABLE or materialized views remain unimplemented. Version-dependent
validation for custom PostgreSQL version identifiers remains unimplemented.
Inheritance and partition propagation are also outside the currently modeled
schema behavior.

### Column type analysis

Standalone semantic analysis of PostgreSQL `ALTER [COLUMN] name [SET DATA]
TYPE type [COLLATE collation] [USING expression]` retains the resolved table
and column, original definition, requested type and modifiers, collation lookup,
and value/default conversion outcomes. Without USING, the old value must permit
assignment conversion. USING refers to the original row, including when several
columns change in one statement. Existing defaults are checked independently;
USING does not convert a default. Repeated type changes for one column are
rejected. Already supported column-attribute operations can accompany a type
change.

This analysis does not apply the change to the schema catalog. Schema-file
application, checking and rebuilding dependent CHECK constraints, foreign keys,
generated columns, views and other dependent definitions remain unimplemented
([#1057](https://github.com/kazuk/sql-fun/issues/1057)). Successful local analysis
does not guarantee successful application: a dependency can cause a later
rejection. Index compatibility and rebuild analysis are tracked separately in
[#1058](https://github.com/kazuk/sql-fun/issues/1058).

With the SQLite feature, the parser-retained `ALTER COLUMN old TO new
column-definition` extension retains its source binding and requested definition
but is rejected as unsupported SQLite engine syntax. Analysis does not synthesize
a table-rebuild migration.

### Rename table

`ALTER TABLE … RENAME TO …` updates the analysis catalog for later statements in
explicitly supplied schema files. The new name stays in the resolved table's
schema; table, column, and result metadata remain available through the new
name. Previously analyzed views and resolved dependencies follow the renamed
table. Independent index and sequence names keep their existing names. The old
table name becomes available for a new declaration.

Analysis retains rejected renames and their diagnostics without changing the
catalog. PostgreSQL `IF EXISTS` skips an absent table. SQLite support requires
the `sqlite` feature and uses its case-insensitive identifier rules.

Foreign keys retain the referenced relation chosen when they were analyzed,
including unqualified references; a later search-path change does not change
that binding. Explicit SQLite `legacy_alter_table`/`foreign_keys` PRAGMA changes
remain unmodeled for rename propagation
([#811](https://github.com/kazuk/sql-fun/issues/811)).

### Rename column

`ALTER TABLE … RENAME [COLUMN] old_name TO new_name` changes the column name for
later statements in explicitly supplied schema files. The column keeps its
position, type, nullability, defaults and other analyzed attributes. Retained
constraints, foreign keys, index references and dependent query bindings follow
the column; independently named objects and explicit query aliases keep their
names. PostgreSQL view output names remain fixed.

An absent source column or occupied destination produces a diagnostic and leaves
the catalog unchanged. PostgreSQL rejects renaming a column to itself and
supports `IF EXISTS` for an absent table. SQLite accepts same-name renames and
uses case-insensitive identifier lookup. With the `sqlite` feature, unaliased
outputs that directly reference the renamed source column follow it; a rename
that makes retained view or trigger references ambiguous or invalid is rejected
atomically. SQLite support remains incomplete: renames that require updating
expression-derived implicit output names, such as `a+1`
([#867](https://github.com/kazuk/sql-fun/issues/867)), or reconstructing
wildcard outputs after `NATURAL JOIN` column matching changes
([#868](https://github.com/kazuk/sql-fun/issues/868)), stop with an explicit
analysis failure. These SQLite-only paths do not affect
PostgreSQL column renames.

Propagation covers dependencies already retained by semantic analysis. This
includes table-level CREATE TABLE foreign keys: both named and unnamed composite
foreign keys retain their resolved relation and column bindings. Inheritance and
partition analysis ([#546](https://github.com/kazuk/sql-fun/issues/546)) remains
separate work; dependencies not yet analyzed cannot participate in renaming.

## Dialect and engine-specific behavior

The column-attribute operations above are PostgreSQL operations. Table and
column renames also have SQLite paths behind the `sqlite` feature, with the
dialect-specific behavior and unsupported propagation cases described above.
Other ALTER TABLE grammar may be retained without completed semantic or catalog
application. In particular, successful ADD COLUMN application remains tracked in
[#528](https://github.com/kazuk/sql-fun/issues/528). Column type changes have the
analysis-only support and application limitations described above.
PostgreSQL 18 constraint enforcement/inheritance transitions remain
unimplemented ([#887](https://github.com/kazuk/sql-fun/issues/887)).

### Other unimplemented alterations

The following retained PostgreSQL operations do not yet have completed
semantic resolution and catalog application; they can reach unimplemented
paths. The supported column attributes and renames above are separate cases.

| Operation | Missing behavior | Tracking issue |
| --- | --- | --- |
| DROP COLUMN | Column removal and dependent catalog changes; SQLite application is also unimplemented | [#531](https://github.com/kazuk/sql-fun/issues/531) |
| ALTER / VALIDATE / DROP CONSTRAINT | Constraint state, deferrability and removal changes | [#533](https://github.com/kazuk/sql-fun/issues/533) |
| Relation physical/storage settings | DROP CLUSTER, SET LOGGED/UNLOGGED, version-applicable DROP OIDS, access method, tablespace and relation options | [#534](https://github.com/kazuk/sql-fun/issues/534) |
| ENABLE / DISABLE TRIGGER or RULE | ALWAYS/REPLICA and ALL/USER state changes where applicable | [#535](https://github.com/kazuk/sql-fun/issues/535) |
| INHERIT / NO INHERIT and OF / NOT OF | Inheritance and typed-table associations | [#536](https://github.com/kazuk/sql-fun/issues/536) |
| REPLICA IDENTITY | Mode and optional index binding/application | [#537](https://github.com/kazuk/sql-fun/issues/537) |
| ENABLE / DISABLE / FORCE / NO FORCE ROW LEVEL SECURITY | Relation row-security state | [#538](https://github.com/kazuk/sql-fun/issues/538) |
| ATTACH / DETACH PARTITION and DETACH PARTITION FINALIZE | Bounds, concurrency and parent/child relationships | [#539](https://github.com/kazuk/sql-fun/issues/539) |
| ADD / SET / DROP IDENTITY | Identity-column transitions and sequence options | [#540](https://github.com/kazuk/sql-fun/issues/540) |

ALTER FOREIGN TABLE analysis also remains unimplemented
([#541](https://github.com/kazuk/sql-fun/issues/541)); ordinary-table support
does not establish foreign-table alterations. ALTER TYPE's composite-type
analysis has its own [unsupported
analysis](create-type.md#dialect-and-engine-specific-behavior).

ALTER ADD COLUMN remains unimplemented for PostgreSQL and SQLite: even an
ordinary column addition reaches an unimplemented catalog-application path.
This includes columns with inline CHECK
([#528](https://github.com/kazuk/sql-fun/issues/528)). Supported CREATE inline
CHECK analysis does not establish ALTER ADD COLUMN support.

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
- [relation_not_found](../../diagnostics/relation_not_found.md): a required
  table target cannot be resolved.
- [column_not_found_in_table](../../diagnostics/column_not_found_in_table.md): a
  required column binding fails.
- [column_already_exists](../../diagnostics/column_already_exists.md): a
  requested destination column name is occupied.
- [conflicting_creation](../../diagnostics/conflicting_creation.md): a table
  rename destination, retained constraint or implicit index conflicts with an
  existing identity.
- [alter_column_definition_syntax](../../diagnostics/alter_column_definition_syntax.md):
  SQLite does not execute the retained column replacement extension.
- [alter_column_duplicate_type](../../diagnostics/alter_column_duplicate_type.md):
  a column has repeated type changes in one statement.
- [alter_column_type_conversion](../../diagnostics/alter_column_type_conversion.md):
  a row value cannot be assigned to the destination type.
- [alter_column_default_conversion](../../diagnostics/alter_column_default_conversion.md):
  the existing default cannot be assigned to the destination type.
- [alter_column_using_expression](../../diagnostics/alter_column_using_expression.md):
  USING contains a prohibited expression or unresolved reference.
- [alter_column_invalid_typmod](../../diagnostics/alter_column_invalid_typmod.md):
  the requested type modifier is invalid.
- [alter_column_missing_relation](../../diagnostics/alter_column_missing_relation.md):
  a column-attribute target relation is absent.
- [alter_column_missing_column](../../diagnostics/alter_column_missing_column.md):
  a column-attribute target column is absent.
- [alter_column_primary_key](../../diagnostics/alter_column_primary_key.md):
  DROP NOT NULL would remove primary-key nullability.
- [alter_column_identity](../../diagnostics/alter_column_identity.md): DROP NOT
  NULL would remove identity-column nullability.
- [alter_column_not_stored_generated](../../diagnostics/alter_column_not_stored_generated.md):
  DROP EXPRESSION is requested for an ineligible non-stored-generated column.
- [alter_column_virtual_generated](../../diagnostics/alter_column_virtual_generated.md):
  a checked operation is inapplicable to a virtual generated column.
- [alter_column_statistics_too_low](../../diagnostics/alter_column_statistics_too_low.md):
  a statistics target is below -1.
- [alter_column_statistics_reduced](../../diagnostics/alter_column_statistics_reduced.md):
  a statistics target above 10000 is clamped, with a warning.
- [alter_column_version](../../diagnostics/alter_column_version.md): SET
  STATISTICS DEFAULT is not established as supported by the explicitly
  configured version.
- [alter_column_unknown_option](../../diagnostics/alter_column_unknown_option.md):
  SET names an unsupported column option.
- [alter_column_duplicate_option](../../diagnostics/alter_column_duplicate_option.md):
  SET repeats a column option.
- [alter_column_invalid_option](../../diagnostics/alter_column_invalid_option.md):
  a supplied column-option value is invalid.
- [alter_column_reset_value](../../diagnostics/alter_column_reset_value.md):
  RESET incorrectly supplies a value.
