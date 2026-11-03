# binary_operator_overload_resolution_failed

## Code and severity

Code: `binary_operator_overload_resolution_failed`. Severity: Error.

## When it is reported

A binary operator name has definitions, but no candidate accepts the two
supplied operand types through supported implicit conversions.

## What analysis knows and does not know

The operator candidates and both operand types are retained. There is no
selected applicable overload; a separately reported `undefined_operator` may
summarize the unusable operation.

## How to address it

Check the operator and both operand types. Use a supported explicit cast only
when it expresses the intended comparison or operation.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT 1 LIKE '1' AS result;
```

## Related diagnostics

[binary_operator_overload_ambiguous](binary_operator_overload_ambiguous.md) has
several equally preferred candidates;
[undefined_operator](undefined_operator.md) reports an unusable operator
expression.
