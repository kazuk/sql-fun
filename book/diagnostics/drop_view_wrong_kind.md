# drop_view_wrong_kind

## Code and severity

Code: `drop_view_wrong_kind`. Severity: Error.

## When it is reported

A PostgreSQL `DROP VIEW` resolves a requested name to a different kind of
relation, such as a table or a materialized view. This also applies with
`IF EXISTS`.

## What analysis knows and does not know

The diagnostic names the visible relation and its kind. The drop leaves
all targets available.

## How to address it

Check the relation name and its schema. Use the drop statement for the
resolved relation kind if that is the relation you intend to remove.

## Example

```sql
CREATE TABLE public.items (id integer);
DROP VIEW IF EXISTS public.items;
```

## Related diagnostics

See [PostgreSQL view removal](../sql-analysis/statements.md#postgresql-view-removal).
