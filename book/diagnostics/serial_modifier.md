# serial_modifier

Code: `serial_modifier`. Severity: Error.

## Meaning

A PostgreSQL serial-family declaration includes a type modifier.

Serial declarations use `smallint`, `integer` or `bigint`. These integer
types do not accept type modifiers.

## How to fix

Remove the type modifier, for example use `serial` instead of `serial(3)`.

## Example

This declaration reports `serial_modifier`:

```sql
CREATE TABLE users (id serial(3));
```

See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
