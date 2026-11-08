# alter_column_default_conversion

## Code and severity

Code: `alter_column_default_conversion`. Severity: Error.

## When it is reported

The existing column default cannot be assigned to the requested type.

## What analysis knows and does not know

USING converts row values, not the existing default. Default conversion is
analyzed separately.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Drop or replace the default separately, then request the type change.

## Example

PostgreSQL; the selected catalog contains public.t(a integer DEFAULT 1). The
example requires an analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TYPE boolean USING true;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
