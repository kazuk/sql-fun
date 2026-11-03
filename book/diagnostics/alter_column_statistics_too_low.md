# alter_column_statistics_too_low

## Code and severity

Code: `alter_column_statistics_too_low`. Severity: Error.

## When it is reported

SET STATISTICS supplies an integer smaller than -1.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The rejected requested value is retained.

## How to address it

Use -1 to restore the default, or a nonnegative target. Choose the value
according to the intended statistics policy.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET STATISTICS -2;
```

-2 is below the accepted lower bound; -1 is the special default request.

## Related diagnostics

[alter_column_statistics_reduced](alter_column_statistics_reduced.md) is a
warning for values above the upper bound.
