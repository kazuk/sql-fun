# array_missing_type_facts

## Code and severity

Code: `array_missing_type_facts`. Severity: Error.

## When it is reported

ARRAY common-type selection needs type-category metadata that the selected
catalog does not provide.

## What analysis knows and does not know

The type requiring metadata is retained. Missing facts do not establish that the
elements are incompatible.

## How to address it

Check that the selected builtin catalog and project type definitions match the
query. Supply the missing supported type metadata; do not change values solely
because analysis lacks facts.

## Example

Illustrative PostgreSQL input: `typed_values.a` and `b` must have distinct
resolved types whose category metadata is unavailable. This condition depends on
catalog contents; this SQL alone does not reproduce it with the normal builtin
catalog.

```sql
SELECT ARRAY[a, b] FROM typed_values;
```

## Related diagnostics

[min_max_missing_type_facts](min_max_missing_type_facts.md) concerns the same
missing category facts in GREATEST/LEAST;
[array_incompatible_types](array_incompatible_types.md) establishes a type
conflict.
