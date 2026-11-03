# CREATE TRIGGER / DROP TRIGGER

## Overview

PostgreSQL and SQLite `CREATE TRIGGER` analysis produces a dedicated semantic
statement, available through `SemAst::trigger()`. It retains the resolved
target, event columns, timing, row/statement granularity, WHEN expression,
applicable constraint and transition-table options, and catalog dependencies.
PostgreSQL function references have a verified zero-input signature returning
`trigger`; arguments written in `EXECUTE FUNCTION (...)` are retained as
protocol strings. The analyzer does not execute or inspect the function body.
SQLite command bodies retain their analyzed SELECT/INSERT/UPDATE/DELETE
statements in order, including references to available OLD/NEW columns in nested
expressions.

Analysis and catalog application are separate. Applying a valid definition
registers it by `TriggerIdentity` and associates its fully qualified name with
the target table or view. PostgreSQL identities include the owning relation;
SQLite identities use the trigger schema and case-insensitive name. A TEMP
SQLite trigger is registered in `temp`, even when its target is in `main`. `IF
NOT EXISTS` preserves the existing definition; PostgreSQL `OR REPLACE` replaces
an eligible existing definition without adding a second association.

Use a table's `triggers()` or an ordinary view's `triggers()` to read the stored
names. `TriggerReferences::resolve(context, target)` resolves them through the
same effective context as direct `BaseParseContext::trigger(identity)` lookup.
Changes in a child schema layer preserve frozen parents and sibling contexts.
Registering these definitions does not itself calculate view writability.

SQLite accepts unknown `UPDATE OF` column names. Each event column records
whether lookup succeeded and its type when known; an unknown name is not a
fabricated column binding. Trigger expressions and SQLite body commands share
the existing expression/DML support limits. SQLite RAISE expression analysis
([#495](https://github.com/kazuk/sql-fun/issues/495)) and whole-row expressions
such as PostgreSQL `WHEN (NEW IS NOT NULL)`
([#614](https://github.com/kazuk/sql-fun/issues/614)) remain unsupported.
Unsuccessful analysis does not register a trigger or attach its name to a
target. PostgreSQL `DROP TRIGGER` resolves the ON relation and its
relation-scoped trigger identity. `SemAst::trigger_drop()` exposes the resolved
identity, dependency behavior and whether lookup found a definition. `IF EXISTS`
retains an explicit no-op outcome for a missing trigger or relation. Applying a
successful drop masks inherited definitions and removes the target reference in
the current layer; re-creation replaces the mask.

SQLite `DROP TRIGGER` uses the schema-scoped trigger namespace, comparing ASCII
identifiers without case sensitivity. An unqualified name searches TEMP, main,
and the explicitly configured attached-schema order. Configure attached schemas
in their SQLite ATTACH order: the earliest attached database wins when names
collide across attachments. A qualified name selects that schema only. The
resolved trigger supplies its owning relation; no ON relation or dependency
behavior is invented. `IF EXISTS` retains an explicit no-op when lookup finds no
definition. Application masks inherited definitions and removes the target link
in the child, preserving parent and sibling catalogs.

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
retained failed lookups, not automatically invalid definitions.
