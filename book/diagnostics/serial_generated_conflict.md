# serial_generated_conflict

Code: `serial_generated_conflict`. Severity: Error.

## Meaning

A generated expression conflicts with a serial column's sequence-backed default.

A PostgreSQL serial-family declaration supplies its own default and cannot
also define a generated expression.

## How to fix

Keep the serial declaration and remove the generated expression, or declare
`smallint`, `integer` or `bigint` with the intended generated expression.

## Example

This declaration reports `serial_generated_conflict`:

```sql
CREATE TABLE users (id serial GENERATED ALWAYS AS (1) STORED);
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
