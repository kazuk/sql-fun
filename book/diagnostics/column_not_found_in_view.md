# column_not_found_in_view

## Code and severity

Code: `column_not_found_in_view`. Severity: Error.

## When it is reported

A reference binds to a view source but its requested output column is absent.

## What analysis knows and does not know

The view identity and requested column are retained. Analysis uses the loaded
view outputs, which may differ from the current database definition.

## How to address it

Use an exposed output name or refresh the explicitly selected schema when its
view definition is stale. Check view column aliases.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer)` and `CREATE
VIEW docs_view AS SELECT id FROM docs_items`.

```sql
SELECT v.missing FROM docs_view AS v;
```

The view exposes `id`; `v.missing` cannot bind.

## Related diagnostics

[view_not_found](view_not_found.md) reports the catalog view itself cannot be
found. [column_not_found_in_table](column_not_found_in_table.md) reports a table
column is missing.
