# explicit_cast_not_defined

## Code and severity

Code: `explicit_cast_not_defined`. Severity: Error.

## When it is reported

A resolved PostgreSQL cast target has no supported explicit conversion from the
independently known source type, including an unsuccessful structural row
conversion.

## What analysis knows and does not know

Both source and destination types are retained. A known target type does not
guarantee that a value can be cast to it.

## How to address it

Select a supported conversion or a source expression of the intended type. For
row casts, match the destination structure and field conversions.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. Cast availability
is dialect-specific; this PostgreSQL check does not establish SQLite cast
behavior.

```sql
SELECT true::point;
```

## Related diagnostics

[implicit_cast_not_found](implicit_cast_not_found.md) concerns an implicit
context; [column_type_not_found](column_type_not_found.md) concerns unresolved
type names.
