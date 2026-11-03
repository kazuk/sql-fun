# column_not_found_in_table

## Code and severity

Code: `column_not_found_in_table`. Severity: Error.

## When it is reported

A reference or table operation requires a column that is absent from a resolved
table.

## What analysis knows and does not know

The table identity and column name are retained. The table was found in the
selected schema; absence there does not prove absence from the live database.

## How to address it

Check column spelling, explicit table aliases and schema selection. Include the
intended column in the loaded table definition when the database schema has
changed.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer, value text)`.
Shared table binding also serves SQLite constructs.

```sql
SELECT t.missing FROM docs_items AS t;
```

The table exists, but has no `missing` column. Use `t.id` if that is the
intended value.

## Related diagnostics

[relation_not_found](relation_not_found.md) reports the relation itself did not
resolve. [column_not_found_in_view](column_not_found_in_view.md) reports the
resolved source is a view.
