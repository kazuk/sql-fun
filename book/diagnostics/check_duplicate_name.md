# check_duplicate_name

Code: `check_duplicate_name`. Severity: Error.

## Meaning

An explicit PostgreSQL CHECK constraint name is already used on that
table or is repeated in the same ALTER statement.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Choose a distinct explicit constraint name or omit the name. Multiple unnamed
CHECK constraints remain distinct. This diagnostic does not claim that
SQLite prohibits repeated constraint names.

## Example

```sql
CREATE TABLE sample(value integer,
    CONSTRAINT positive CHECK(value > 0),
    CONSTRAINT positive CHECK(value < 100));
```

See [CHECK constraints](../sql-analysis/statements/create-table.md#check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
