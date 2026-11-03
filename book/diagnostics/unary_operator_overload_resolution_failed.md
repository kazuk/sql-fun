# unary_operator_overload_resolution_failed

## Code and severity

Code: `unary_operator_overload_resolution_failed`. Severity: Error.

## When it is reported

A prefix or postfix operator has definitions, but no overload accepts the known
operand type through supported implicit conversion.

## What analysis knows and does not know

The candidate definitions and operand type are retained. No applicable operator
is selected.

## How to address it

Use an operator defined for the intended operand type, or a meaningful supported
explicit conversion.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. The example also
reports `undefined_operator`.

```sql
SELECT -true;
```

## Related diagnostics

[unary_operator_overload_ambiguous](unary_operator_overload_ambiguous.md) has
multiple best candidates;
[binary_operator_overload_resolution_failed](binary_operator_overload_resolution_failed.md)
concerns two operands.
