# serial_default_conflict

Code: `serial_default_conflict`. Severity: Error.

## Meaning

An explicit DEFAULT conflicts with a serial column's sequence-backed default.

A PostgreSQL serial-family declaration supplies its own default that obtains
a value from the associated sequence.

## How to fix

Remove the explicit DEFAULT to use serial's sequence. To specify a different
default, declare `smallint`, `integer` or `bigint` explicitly.

## Example

This declaration reports `serial_default_conflict`:

```sql
CREATE TABLE users (id serial DEFAULT 1);
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
