# query_set_column_count

## Code and severity

Code: `query_set_column_count`. Severity: Error.

## When it is reported

The operands of a set operation expose different numbers of columns.

## What analysis knows and does not know

The operation and both operand widths are retained. Column type compatibility is
a separate check after shape.

## How to address it

Select the same number of columns on both sides in corresponding positions. Use
a deliberately typed placeholder only if it is part of the intended result
shape.

## Example

PostgreSQL 18 builtin catalog; UNION, INTERSECT and EXCEPT require compatible
row widths.

```sql
SELECT 1 UNION SELECT 1, 2;
```

The left side has one column and the right side has two. For one output, use
`SELECT 1 UNION SELECT 2`.

## Related diagnostics

[query_set_column_type](query_set_column_type.md) reports equal-width operands
have incompatible positional types. [membership_width](membership_width.md)
reports membership operand widths differ.
