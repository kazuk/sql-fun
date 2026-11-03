# alter_column_missing_column

## Code and severity

Code: `alter_column_missing_column`. Severity: Error.

## When it is reported

The ALTER COLUMN target table is bound, but the named column is not present.

## What analysis knows and does not know

The table identity and requested column name are known; no column definition was
bound.

## How to address it

Correct the column name or load the schema version containing the intended
column.

## Example

PostgreSQL; the selected project schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN absent SET NOT NULL;
```

The relation exists; only the requested column is absent.

## Related diagnostics

[alter_column_missing_relation](alter_column_missing_relation.md) concerns
failure to bind the table itself.
