# distinct_row_arity

## Code and severity

Code: `distinct_row_arity`. Severity: Error.

## When it is reported

The explicit row constructors on the two sides of IS DISTINCT FROM or IS NOT
DISTINCT FROM have different field counts.

## What analysis knows and does not know

Both field counts are retained. This check does not require determining a common
type for mismatched positions.

## How to address it

Compare rows with equal numbers of fields, in the intended corresponding order.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT ROW(1, 2) IS DISTINCT FROM ROW(1) AS result;
```

## Related diagnostics

[distinct_equality_result](distinct_equality_result.md) concerns the selected
equality result; [membership_width](membership_width.md) concerns IN/NOT IN
operand widths.
