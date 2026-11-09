# `drop_column_dependency`

## What causes it

A known dependent object prevents the requested column removal. PostgreSQL
requires CASCADE for some dependencies, including dependent views and external
foreign keys. SQLite rejects removals that invalidate retained schema objects;
it has no DROP COLUMN CASCADE option.

## How to address it

Inspect the dependent object named in the diagnostic. On PostgreSQL, explicitly
choose CASCADE only when you intend to remove the dependencies. On SQLite,
change the dependent schema before removing the column.

## Example

With a view that selects `sample.a`:

```sql
ALTER TABLE sample DROP COLUMN a RESTRICT;
```

## Related diagnostics

See [DROP COLUMN analysis](../sql-analysis/statements/alter-table.md#column-removal-analysis).
