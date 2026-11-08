# `add_column_sqlite_key`

## What causes it

SQLite ADD COLUMN cannot declare PRIMARY KEY or UNIQUE.
The entire ALTER statement leaves the analysis catalog unchanged.

## How to address it

Use a column definition supported by the selected database and compatible with
the existing table. See the SQL Analysis Reference for the supported scope.

## Example

The selected SQLite catalog contains `sample`; SQLite support requires the
`sqlite` feature. The SQL parser can reject this restriction before semantic
analysis.

```sql
ALTER TABLE sample ADD COLUMN id INTEGER UNIQUE;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md#adding-columns).
