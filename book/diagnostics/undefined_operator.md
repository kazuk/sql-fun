# undefined_operator

## Code and severity

Code: `undefined_operator`. Severity: Error.

## When it is reported

An operator expression has no usable definition for its analyzed operands. It
can accompany more specific overload diagnostics.

## What analysis knows and does not know

The operator name and available operand facts are retained. An unresolved
operand type differs from a proven mismatch; check accompanying diagnostics
before choosing a correction.

## How to address it

Check operand names and types first, then select a supported operator or
intentional conversion. Preserve valid unsupported operations when reporting an
analysis limitation.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. The example also
reports `binary_operator_overload_resolution_failed`.

```sql
SELECT 1 LIKE '1' AS result;
```

## Related diagnostics

[binary_operator_overload_resolution_failed](binary_operator_overload_resolution_failed.md)
gives typed binary candidate failure;
[expression_type_not_known](expression_type_not_known.md) lacks an operand type.
