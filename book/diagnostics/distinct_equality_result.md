# distinct_equality_result

## Code and severity

Code: `distinct_equality_result`. Severity: Error.

## When it is reported

The equality operator selected for a distinctness comparison returns a known
non-boolean type.

## What analysis knows and does not know

The selected result type is retained. A valid null-safe comparison requires a
boolean equality operation; this differs from missing or ambiguous operator
selection.

## How to address it

Check custom equality operators and their declared return types. Select operands
whose equality operator has the intended boolean result.

## Example

Illustrative PostgreSQL input: `operands.a` and `b` must resolve to types whose
selected equality operator returns a non-boolean type. Ordinary builtin integer
equality does not reproduce this metadata-dependent condition.

```sql
SELECT a IS DISTINCT FROM b FROM operands;
```

## Related diagnostics

[distinct_row_arity](distinct_row_arity.md) concerns row widths;
[membership_comparison_result](membership_comparison_result.md) checks a
membership comparison result.
