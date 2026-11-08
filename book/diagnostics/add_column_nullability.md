# `add_column_nullability`

## What causes it

An added PostgreSQL column declares both NULL and NOT NULL, including NOT NULL implied
by PRIMARY KEY. The entire statement leaves the analysis catalog unchanged.

## How to address it

Choose one nullability for the added column. A primary-key column cannot be nullable.

## Example

```sql
ALTER TABLE sample ADD COLUMN id integer PRIMARY KEY NULL;
```

## Related diagnostics

See [ALTER TABLE](../sql-analysis/statements/alter-table.md#adding-columns).
