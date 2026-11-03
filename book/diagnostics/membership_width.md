# membership_width

## Code and severity

Code: `membership_width`. Severity: Error.

## When it is reported

An IN or NOT IN comparison has mismatched operand widths, including row/list or
subquery result widths.

## What analysis knows and does not know

The expected and actual widths are retained. The diagnostic concerns columns or
fields, not the number of rows searched.

## How to address it

Match the subquery projection or row-list entry width to the left-hand operand.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT 1 IN (SELECT 1, 2) AS result;
```

## Related diagnostics

[membership_comparison_result](membership_comparison_result.md) concerns
comparison type; [distinct_row_arity](distinct_row_arity.md) concerns explicit
distinctness rows.
