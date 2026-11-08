# alter_column_using_expression

## Code and severity

Code: `alter_column_using_expression`. Severity: Error.

## When it is reported

USING contains an unresolved reference, a parameter, a subquery, an aggregate, a
window function or a set-returning expression.

## What analysis knows and does not know

USING is analyzed against the original table row. Its retained operand is
locally rejected; no catalog update is performed.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Use a scalar expression referring to existing columns of the original row.

## Example

PostgreSQL; the selected catalog contains public.t(a integer), with no column
named absent. The example requires an analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TYPE bigint USING absent;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
