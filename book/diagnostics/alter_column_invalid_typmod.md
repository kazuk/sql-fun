# alter_column_invalid_typmod

## Code and severity

Code: `alter_column_invalid_typmod`. Severity: Error.

## When it is reported

The requested destination type has invalid type modifiers.

## What analysis knows and does not know

The modifier spelling is retained and checked against supported declaration
rules. No live row values are examined.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Correct the modifier arguments for the requested type.

## Example

PostgreSQL; the selected catalog contains public.t(a integer). The example
requires an analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TYPE numeric(0) USING NULL;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
