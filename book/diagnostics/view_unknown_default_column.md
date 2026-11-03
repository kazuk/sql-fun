# view_unknown_default_column

## Code and severity

Code: `view_unknown_default_column`. Severity: Error.

## When it is reported

An ALTER VIEW default assignment or removal names a column that is not part of
the view result.

## What analysis knows and does not know

The view identity and requested column are retained. The view can exist while
exposing a different name from its underlying table.

## How to address it

Use the view's exposed column name, accounting for aliases or an explicit view
column list.

## Example

PostgreSQL; the selected schema contains `public.v AS SELECT a FROM public.t`.

```sql
ALTER VIEW public.v ALTER COLUMN absent SET DEFAULT 1;
```

The view exposes a, not absent; defaults refer to the view output names.

## Related diagnostics

[view_invalid_target](view_invalid_target.md) means the required view itself
could not be bound.
