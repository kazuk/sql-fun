# `add_column_default`

## What causes it

An added default contains an unresolved or prohibited expression, or its type
cannot be assigned to the declared PostgreSQL column type.
The entire ALTER statement leaves the analysis catalog unchanged.

## How to address it

Use a column definition supported by the selected database and compatible with
the existing table. See the SQL Analysis Reference for the supported scope.

## Example

The selected catalog contains `sample`.

```sql
ALTER TABLE sample ADD COLUMN enabled boolean DEFAULT 1;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md#adding-columns).
