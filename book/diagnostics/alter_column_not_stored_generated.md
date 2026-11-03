# alter_column_not_stored_generated

## Code and severity

Code: `alter_column_not_stored_generated`. Severity: Error.

## When it is reported

DROP EXPRESSION targets a column without stored generation, and IF EXISTS was
not specified.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved.

## How to address it

Target the intended stored generated column. If skipping a column with no
generation expression is intended, use DROP EXPRESSION IF EXISTS.

## Example

PostgreSQL; the selected schema contains ordinary column `public.t.a` of type
integer.

```sql
ALTER TABLE public.t ALTER COLUMN a DROP EXPRESSION;
```

An ordinary column has no stored generation expression to remove.

## Related diagnostics

[alter_column_virtual_generated](alter_column_virtual_generated.md)
distinguishes virtual generation from the absence of a stored expression.
