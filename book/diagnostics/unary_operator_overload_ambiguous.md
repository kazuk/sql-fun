# unary_operator_overload_ambiguous

## Code and severity

Code: `unary_operator_overload_ambiguous`. Severity: Error.

## When it is reported

Several prefix or postfix operator overloads remain equally preferred for the
known operand type.

## What analysis knows and does not know

The operator candidates and operand type are retained. The diagnostic
establishes ambiguity rather than absence of an applicable operator.

## How to address it

Specify the intended operand type or operator namespace, and check competing
custom overload definitions.

## Example

Illustrative PostgreSQL input: `operands.value` and custom unary `-` definitions
must produce equally ranked candidates. This is a catalog-dependent example, not
a reproduction using the normal integer operator.

```sql
SELECT -value FROM operands;
```

## Related diagnostics

[unary_operator_overload_resolution_failed](unary_operator_overload_resolution_failed.md)
has no applicable candidate;
[binary_operator_overload_ambiguous](binary_operator_overload_ambiguous.md) has
two operands.
