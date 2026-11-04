# serial_identity_conflict

Code: `serial_identity_conflict`. Severity: Error.

## Meaning

An IDENTITY clause conflicts with a serial column's sequence-backed default.

A PostgreSQL serial-family declaration already supplies a sequence-backed
default and cannot also be an identity column.

## How to fix

Choose one form: keep the serial declaration and remove IDENTITY, or declare
`smallint`, `integer` or `bigint` with the intended identity clause.

## Example

This declaration reports `serial_identity_conflict`:

```sql
CREATE TABLE users (id serial GENERATED ALWAYS AS IDENTITY);
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
