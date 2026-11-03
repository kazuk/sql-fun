# incompatible_case_values

## Code and severity

Code: `incompatible_case_values`. Severity: Error.

## When it is reported

CASE result arms, including ELSE, cannot be assigned a common implicit result
type in PostgreSQL analysis.

## What analysis knows and does not know

The ordered result type facts are retained. The diagnostic concerns result arms,
not just whether WHEN conditions are boolean.

## How to address it

Use result values with a common type and meaningful supported conversions. Keep
the chosen result type consistent in every arm.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. SQLite has
different expression typing rules; this PostgreSQL example does not establish
rejection there.

```sql
SELECT CASE WHEN true THEN 1 ELSE false END AS result;
```

## Related diagnostics

[coalesce_expr_type_unknown](coalesce_expr_type_unknown.md) concerns COALESCE
operands; [parameter_constraint](parameter_constraint.md) concerns incompatible
inferred requirements.
