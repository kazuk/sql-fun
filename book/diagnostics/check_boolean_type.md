# check_boolean_type

Code: `check_boolean_type`. Severity: Error.

## Meaning

A PostgreSQL table CHECK condition or one of its boolean operands has a type
that cannot be coerced to boolean.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Write a boolean condition such as a comparison or null test. An integer or
explicitly typed text expression is not a PostgreSQL boolean condition.
SQLite CHECK truth-value rules do not require PostgreSQL boolean coercion.

## Example

```sql
CREATE TABLE sample(value integer, CHECK(value));
```

See [table CHECK constraints](../sql-analysis/statements/create-table.md#table-check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
