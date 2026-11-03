# alter_column_statistics_reduced

## Code and severity

Code: `alter_column_statistics_reduced`. Severity: Warning.

## When it is reported

SET STATISTICS requests a value above 10000; analysis reduces the effective
target to 10000.

## What analysis knows and does not know

The table, column and requested value are known. This warning permits the
normalized change when no independent error rejects the statement.

## How to address it

Use at most 10000 if the normalized value is intended, or select a smaller
target. Review any independent errors before relying on the alteration.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET STATISTICS 10001;
```

The effective target is 10000. This is a warning, not a rejection for being too
high.

## Related diagnostics

[alter_column_statistics_too_low](alter_column_statistics_too_low.md) is an
error for a value below -1.
[index_statistics_clamped](index_statistics_clamped.md) concerns an index
expression column.
