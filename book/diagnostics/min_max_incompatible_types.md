# min_max_incompatible_types

## Code and severity

Code: `min_max_incompatible_types`. Severity: Error.

## When it is reported

GREATEST/LEAST operands cannot be matched and implicitly converted to the
selected common type.

## What analysis knows and does not know

The conflicting operand types are retained. The failure can arise from different
type categories or a missing required conversion.

## How to address it

Use compatible operand types or meaningful supported explicit casts. These
expressions must agree on one common result type before ordering is checked.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This is PostgreSQL
GREATEST/LEAST syntax, not SQLite scalar min/max.

```sql
SELECT GREATEST(1, true);
```

## Related diagnostics

[min_max_not_ordered](min_max_not_ordered.md) concerns comparison support after
common-type selection; [array_incompatible_types](array_incompatible_types.md)
concerns ARRAY elements.
