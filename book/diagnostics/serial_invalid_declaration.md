# serial_invalid_declaration

Code: `serial_invalid_declaration`. Severity: Error.

## Meaning

A PostgreSQL serial-family column declaration conflicts with its implicit
integer type, NOT NULL constraint or sequence-backed default.

## How to fix

Use a scalar serial declaration without type modifiers. Remove explicit NULL
or conflicting DEFAULT, IDENTITY and generated expressions. If you want to
specify those properties yourself, declare the underlying `smallint`,
`integer` or `bigint` type and its intended attributes explicitly.

```sql
CREATE TABLE users (id bigserial PRIMARY KEY);
```

Serial is PostgreSQL column-declaration shorthand, not a general catalog type.
See [CREATE TABLE](../sql-analysis/statements/create-table.md#postgresql-serial-columns).
