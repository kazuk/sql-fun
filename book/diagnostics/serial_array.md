# serial_array

Code: `serial_array`. Severity: Error.

## Meaning

A PostgreSQL serial-family column is declared as an array.

Serial declarations create scalar integer columns with sequence-backed defaults.
PostgreSQL does not support arrays of serial.

## How to fix

Use a scalar serial declaration. If you need an integer array, declare
`smallint[]`, `integer[]` or `bigint[]` and its intended default explicitly.

## Example

This declaration reports `serial_array`:

```sql
CREATE TABLE users (id serial[]);
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
