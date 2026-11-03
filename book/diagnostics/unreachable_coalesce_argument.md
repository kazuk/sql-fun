# unreachable_coalesce_argument

## Code and severity

Code: `unreachable_coalesce_argument`. Severity: Warning. The warning is
selected when an earlier operand is proven non-null; unknown nullability alone
does not establish unreachability.

## When it is reported

An argument of COALESCE follows an operand that analysis has proven non-null, so
evaluation would never reach the later argument.

## What analysis knows and does not know

The earlier and later operand facts are retained, including types and
nullability where known. This is separate from checking whether all operands
have compatible types. A warning does not establish that the complete expression
is valid.

## How to address it

Remove the unused operand if it is unnecessary, or revise the earlier expression
if a nullable fallback was intended. Also address any independent type
diagnostic for the complete COALESCE expression.

## Example

PostgreSQL 17–18 builtin schema.

```sql
SELECT COALESCE(1, true);
```

`1` is proven non-null, making `true` unreachable. This example also reports
`coalesce_expr_type_unknown` because the operand types cannot be combined;
unreachability does not suppress that type check. If a constant integer is
intended, `SELECT 1` avoids the unused fallback.

## Related diagnostics

[coalesce_expr_type_unknown](coalesce_expr_type_unknown.md) reports inability to
establish the common operand type.
[expression_type_not_known](expression_type_not_known.md) concerns an unresolved
expression type rather than a proven non-null operand.
