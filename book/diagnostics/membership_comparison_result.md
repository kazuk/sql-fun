# membership_comparison_result

## Code and severity

Code: `membership_comparison_result`. Severity: Error.

## When it is reported

A bound IN/NOT IN comparison produces a known non-boolean type.

## What analysis knows and does not know

The comparison result type is retained. Operand widths can be valid while the
selected operator result is unsuitable.

## How to address it

Check custom comparison operators and their return declarations. Use operand
types with an applicable boolean comparison.

## Example

Illustrative PostgreSQL input: the selected comparison for the resolved operand
types must return a non-boolean result. Normal builtin integer equality does not
reproduce this catalog-dependent condition.

```sql
SELECT value IN (other_value) FROM operands;
```

## Related diagnostics

[membership_width](membership_width.md) concerns shape;
[distinct_equality_result](distinct_equality_result.md) checks the equality used
for distinctness.
