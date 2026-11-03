# coalesce_expr_type_unknown

## Code and severity

Code: `coalesce_expr_type_unknown`. Severity: Error.

## When it is reported

COALESCE operands cannot be combined into a common implicit result type.

## What analysis knows and does not know

The ordered operand type facts are retained. Unreachable operands are still
subject to common-type validation; nullability alone does not settle type
compatibility.

## How to address it

Use compatible operand types. Remove an unused fallback when that preserves the
intended query, or explicitly convert values where a supported conversion is
appropriate.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This example also
reports `unreachable_coalesce_argument`.

```sql
SELECT COALESCE(1, true);
```

## Related diagnostics

[unreachable_coalesce_argument](unreachable_coalesce_argument.md) is a warning
about evaluation reachability;
[incompatible_case_values](incompatible_case_values.md) concerns CASE result
arms.
