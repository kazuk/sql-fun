# CREATE TRIGGER / DROP TRIGGER

## Overview

PostgreSQL and SQLite `CREATE TRIGGER` analysis checks the target, event
columns, timing, row/statement granularity, WHEN expression, and applicable
constraint and transition-table options. PostgreSQL function references have a
verified zero-input signature returning `trigger`; arguments written in `EXECUTE
FUNCTION (...)` are treated as strings, not SQL input expressions. The analyzer
does not execute or inspect the function body. SQLite command bodies are
analyzed as SELECT/INSERT/UPDATE/DELETE statements in written order, including
references to available OLD/NEW columns in nested expressions.

A valid trigger definition becomes available for subsequent schema analysis.
PostgreSQL trigger names belong to the owning relation; SQLite trigger names
belong to a schema and are compared without case sensitivity. A TEMP
SQLite trigger is registered in `temp`, even when its target is in `main`. `IF
NOT EXISTS` preserves the existing definition; PostgreSQL `OR REPLACE` replaces
an eligible existing definition.

SQLite accepts unknown `UPDATE OF` column names. Analysis determines a column's
type only when it exists in the target relation. Trigger expressions and SQLite
body commands share the existing expression/DML support limits. SQLite RAISE
expression analysis ([#495](https://github.com/kazuk/sql-fun/issues/495)) and
whole-row expressions such as PostgreSQL `WHEN (NEW IS NOT NULL)`
([#614](https://github.com/kazuk/sql-fun/issues/614)) remain unsupported. An
invalid trigger definition does not become available to subsequent schema
analysis. PostgreSQL `DROP TRIGGER` resolves the ON relation and its trigger
name within that relation. `IF EXISTS` accepts a missing trigger or relation
without changing the schema. After a successful drop, the trigger is no longer
available to subsequent statements in the same schema analysis.

SQLite `DROP TRIGGER` uses the schema-scoped trigger namespace, comparing ASCII
identifiers without case sensitivity. An unqualified name searches TEMP, main,
and the explicitly configured attached-schema order. Configure attached schemas
in their SQLite ATTACH order: the earliest attached database wins when names
collide across attachments. A qualified name selects that schema only. The
resolved trigger supplies its owning relation; no ON relation or dependency
behavior is invented. `IF EXISTS` retains an explicit no-op when lookup finds no
definition. After a successful drop, the trigger is no longer available to
subsequent statements in the same schema analysis.

## Dialect and engine-specific behavior

PostgreSQL trigger functions and SQLite trigger bodies are different analysis
paths, as described above. Registering or dropping a definition updates only the
effective analysis catalog; it never installs or removes a database trigger. See
[CREATE VIEW](create-view.md#writable-views) for the separate evaluation of
view-write capabilities.

## Diagnostics

- [trigger_invalid_definition](../../diagnostics/trigger_invalid_definition.md):
  a checked target, duplicate identity, timing/event/option combination,
  event-column binding, WHEN condition, trigger function or drop requirement is
  invalid.

SQLite body statements and supported WHEN expressions can emit their shared
statement and expression diagnostics. Unknown SQLite UPDATE OF names are
not automatically invalid trigger definitions.
