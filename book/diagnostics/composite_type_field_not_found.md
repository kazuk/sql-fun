# composite_type_field_not_found

## Code and severity

Code: `composite_type_field_not_found`. Severity: Error.

## When it is reported

Field access targets a composite value, but its known row shape has no field
with the requested name.

## What analysis knows and does not know

The composite type and field name are retained. The analyzer knows a composite
shape, unlike a scalar value that cannot support field access at all.

## How to address it

Use an actual field name or correct the row/type declaration. Check column
aliases for anonymous row shapes.

## Example

PostgreSQL 18; selected schema contains `CREATE TYPE docs_pair AS (id integer)`.

```sql
SELECT (NULL::docs_pair).missing;
```

The composite has field `id`, not `missing`; `(NULL::docs_pair).id` uses the
existing field.

## Related diagnostics

[member_access_requires_composite_type](member_access_requires_composite_type.md)
reports the operand is not known to be composite.
[column_not_found_in_table](column_not_found_in_table.md) reports direct
table-column lookup failed.
