# `drop_column_last_column`

## What causes it

SQLite requires at least one non-generated column; dropping the final such
column is rejected even if generated columns remain. PostgreSQL permits
zero-column tables.

## How to address it

Keep at least one non-generated column in the SQLite table.

## Example

For a table declared as `CREATE TABLE sample (a INTEGER)`:

```sql
ALTER TABLE sample DROP COLUMN a;
```

## Related diagnostics

See [DROP COLUMN analysis](../sql-analysis/statements/alter-table.md#column-removal-analysis).
