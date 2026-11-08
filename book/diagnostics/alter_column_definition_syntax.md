# alter_column_definition_syntax

## Code and severity

Code: `alter_column_definition_syntax`. Severity: Error.

## When it is reported

The SQLite parser retains a column replacement extension that the SQLite engine
does not execute.

## What analysis knows and does not know

Bindings and the requested definition are retained, but the operation is
rejected. Analysis does not generate a table-rebuild migration.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Use SQLite-supported ALTER operations or plan an explicit migration.

## Example

SQLite; the selected catalog contains t(a INTEGER). The example requires an
analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TO b TEXT;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
