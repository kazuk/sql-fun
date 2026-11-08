# `add_column_primary_key`

## What causes it

A PostgreSQL table already has a primary key; an addition would introduce
another one.
The entire ALTER statement leaves the analysis catalog unchanged.

## How to address it

Use a column definition supported by the selected database and compatible with
the existing table. See the SQL Analysis Reference for the supported scope.

## Example

The selected catalog contains `sample` with an existing primary key.

```sql
ALTER TABLE sample ADD COLUMN second_id integer PRIMARY KEY;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md#adding-columns).
