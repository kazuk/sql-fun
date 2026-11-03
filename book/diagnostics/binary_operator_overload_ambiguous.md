# binary_operator_overload_ambiguous

## Code and severity

Code: `binary_operator_overload_ambiguous`. Severity: Error.

## When it is reported

More than one binary operator overload remains equally preferred after candidate
filtering.

## What analysis knows and does not know

The operator definitions and operand types are retained. Analysis cannot
establish a unique overload, even though applicable candidates exist.

## How to address it

Disambiguate the intended operand types or operator namespace. Inspect custom
overload declarations and avoid relying on an arbitrary candidate.

## Example

Illustrative PostgreSQL input: the selected schema must contain `operands` and
competing `+` overloads that accept its column types with equal conversion rank.
Normal integer builtins do not reproduce this ambiguity.

```sql
SELECT left_value + right_value FROM operands;
```

## Related diagnostics

[binary_operator_overload_resolution_failed](binary_operator_overload_resolution_failed.md)
means no candidate applies;
[unary_operator_overload_ambiguous](unary_operator_overload_ambiguous.md)
concerns one-operand operators.
