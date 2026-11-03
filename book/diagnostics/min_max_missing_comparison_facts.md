# min_max_missing_comparison_facts

## Code and severity

Code: `min_max_missing_comparison_facts`. Severity: Error.

## When it is reported

GREATEST/LEAST has a common result type, but the selected catalog cannot
establish whether that type supports the required ordering.

## What analysis knows and does not know

The common type is retained. Ordering is unknown, rather than known to be
unavailable.

## How to address it

Verify that the selected catalog contains supported comparison/operator-class
metadata for the type and its base or element type. A meaningful supported
conversion to a known ordered type can be used when that is the intended
operation.

## Example

Illustrative PostgreSQL input: `typed_values.value` must have a resolved common
type with unavailable comparison metadata. This is catalog-dependent and is not
reproduced by ordinary builtin integer metadata.

```sql
SELECT GREATEST(value, value) FROM typed_values;
```

## Related diagnostics

[min_max_not_ordered](min_max_not_ordered.md) establishes lack of comparison
support; [min_max_missing_type_facts](min_max_missing_type_facts.md) lacks
category information earlier.
