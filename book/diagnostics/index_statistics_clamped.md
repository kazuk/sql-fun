# index_statistics_clamped

## Code and severity

Code: `index_statistics_clamped`. Severity: Warning.

## When it is reported

ALTER INDEX ALTER COLUMN SET STATISTICS requests more than 10000 for a valid
expression-key column.

## What analysis knows and does not know

The index, one-based column ordinal and requested value are retained. The
effective target is 10000, and the warning alone does not prevent application.

## How to address it

Request at most 10000 if that target is intended. Also resolve any independent
errors in the statement.

## Example

PostgreSQL; the schema contains `public.t_expr_idx`, a B-tree index on
expression `(a+1)` in its first key column.

```sql
ALTER INDEX public.t_expr_idx ALTER COLUMN 1 SET STATISTICS 10001;
```

The example requires an expression column; an ordinary key column does not reach
this warning.

## Related diagnostics

[index_invalid_alteration](index_invalid_alteration.md) covers rejected index
changes. [alter_column_statistics_reduced](alter_column_statistics_reduced.md)
concerns table-column statistics.
