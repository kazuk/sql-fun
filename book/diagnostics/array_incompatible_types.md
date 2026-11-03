# array_incompatible_types

## Code and severity

Code: `array_incompatible_types`. Severity: Error.

## When it is reported

ARRAY elements cannot be matched or converted to a common element type,
including incompatible nested-array elements.

## What analysis knows and does not know

The conflicting type pair is retained. The array result type cannot be
established through the required coercions.

## How to address it

Use elements with compatible types, or explicitly convert each value where the
conversion expresses the intended data. An outer cast does not make every
conversion valid.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT ARRAY[1, true];
```

## Related diagnostics

[array_empty](array_empty.md) lacks an element type;
[min_max_incompatible_types](min_max_incompatible_types.md) applies similar
common-type reasoning to GREATEST/LEAST.
