# array_subquery_columns

## Code and severity

Code: `array_subquery_columns`. Severity: Error.

## When it is reported

An ARRAY subquery returns a number of columns other than one, after any pending
wildcard expansion is resolved.

## What analysis knows and does not know

The observed column count is retained. This is a width requirement, not a
restriction on how many rows the subquery returns.

## How to address it

Select exactly one expression in the ARRAY subquery. Use a single row-valued
expression if the intended array elements are records.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT ARRAY(SELECT 1, 2);
```

## Related diagnostics

[query_scalar_value_required](query_scalar_value_required.md) concerns scalar
expression shape; [membership_width](membership_width.md) checks widths of
membership operands.
