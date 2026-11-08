# alter_column_type_conversion

## Code and severity

Code: `alter_column_type_conversion`. Severity: Error.

## When it is reported

The original value, or the analyzed USING result, cannot be assigned to the
requested type.

## What analysis knows and does not know

The target type and source binding are known, but assignment conversion is
unavailable. This does not validate existing database values.

These examples describe standalone semantic analysis. Catalog application of
column type changes remains
[#1057](https://github.com/kazuk/sql-fun/issues/1057).

## How to address it

Supply a USING expression whose result can be assigned to the destination type.

## Example

PostgreSQL; the selected catalog contains public.t(a integer). The example
requires an analysis context
providing the stated catalog facts.

```sql
ALTER TABLE t ALTER COLUMN a TYPE boolean;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md) for the supported
analysis scope and application limitations.
