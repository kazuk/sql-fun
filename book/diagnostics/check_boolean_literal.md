# check_boolean_literal

Code: `check_boolean_literal`. Severity: Error.

## Meaning

An unknown string literal used as a PostgreSQL CHECK boolean input is not a
valid boolean spelling.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Use a valid boolean literal, such as `true` or `false`, or a boolean condition.
Unknown strings can use PostgreSQL boolean input spellings such as `'yes'`;
an ambiguous abbreviation or another invalid spelling cannot be coerced.

## Example

```sql
CREATE TABLE sample(value integer, CHECK('bad'));
```

See [table CHECK constraints](../sql-analysis/statements/create-table.md#table-check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
