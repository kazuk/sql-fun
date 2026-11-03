# member_access_requires_composite_type

## Code and severity

Code: `member_access_requires_composite_type`. Severity: Error.

## When it is reported

A field or whole-row member access receives a type that does not provide the
required composite structure.

## What analysis knows and does not know

The operand type is retained. Depending on the path, an absent or unusable base
definition can also prevent obtaining a composite shape; the message does not
establish that a named field exists.

## How to address it

Use a row/composite operand and verify its declared fields and domain base
types. Remove field access when the intended value is scalar.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT (1).field;
```

## Related diagnostics

[composite_type_field_not_found](composite_type_field_not_found.md) concerns a
missing field in a known composite type; [require_array](require_array.md)
concerns array structure.
