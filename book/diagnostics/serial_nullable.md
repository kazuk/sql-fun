# serial_nullable

Code: `serial_nullable`. Severity: Error.

## Meaning

Explicit NULL conflicts with a serial column's implicit NOT NULL constraint.

PostgreSQL serial-family declarations always imply NOT NULL.

## How to fix

Remove explicit NULL. For a nullable integer column, declare `smallint`,
`integer` or `bigint` with the attributes and default you intend.

## Example

This declaration reports `serial_nullable`:

```sql
CREATE TABLE users (id serial NULL);
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
