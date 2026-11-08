# alter_column_duplicate_type

## Code and severity

Code: `alter_column_duplicate_type`. Severity: Error.

## When it is reported

One ALTER TABLE statement changes the type of the same column more than once.

## What analysis knows and does not know

The repeated column binding is known. The statement is locally rejected before
any catalog change.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Specify one type change per column in the statement.

## Example

PostgreSQL; the selected catalog contains public.t(a integer). The example
requires an analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TYPE bigint, ALTER COLUMN a TYPE bigint;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
