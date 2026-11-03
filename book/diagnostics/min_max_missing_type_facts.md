# min_max_missing_type_facts

## Code and severity

Code: `min_max_missing_type_facts`. Severity: Error.

## When it is reported

GREATEST/LEAST common-type selection needs a type category that the selected
catalog does not establish.

## What analysis knows and does not know

The type lacking category facts is retained. This is missing analysis
information, not proof that operands cannot be combined.

## How to address it

Verify the selected builtin catalog and project types. Supply supported category
metadata rather than treating absent metadata as an incompatible SQL value.

## Example

Illustrative PostgreSQL input: `typed_values` must provide distinct resolved
operand types with unavailable category facts. Ordinary builtin numeric columns
do not reproduce this condition.

```sql
SELECT GREATEST(a, b) FROM typed_values;
```

## Related diagnostics

[array_missing_type_facts](array_missing_type_facts.md) concerns ARRAY category
metadata; [min_max_incompatible_types](min_max_incompatible_types.md)
establishes a conflict.
