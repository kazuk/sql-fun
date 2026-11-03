# require_array

## Code and severity

Code: `require_array`. Severity: Error.

## When it is reported

An operation requiring an array receives a known type whose array capability is
false.

## What analysis knows and does not know

The expression facts and type are retained. Unknown type information alone does
not trigger this non-array check.

## How to address it

Pass an actual array expression, or correct the operation if a scalar value was
intended.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT (1)[1];
```

## Related diagnostics

[array_index_type_mismatch](array_index_type_mismatch.md) concerns the
subscript; [array_empty](array_empty.md) concerns determining an ARRAY element
type.
