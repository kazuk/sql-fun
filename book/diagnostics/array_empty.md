# array_empty

## Code and severity

Code: `array_empty`. Severity: Error.

## When it is reported

An untyped ARRAY constructor has no elements from which to determine its element
type.

## What analysis knows and does not know

The constructor is known to be empty. No element type is established; an empty
array is different from SQL NULL.

## How to address it

Specify the intended array type with an explicit SQL cast, for example
`ARRAY[]::int4[]`.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT ARRAY[];
```

## Related diagnostics

[array_incompatible_types](array_incompatible_types.md) concerns conflicting
element types, rather than absent elements.
