# check_expression_placement

Code: `check_expression_placement`. Severity: Error.

## Meaning

A table CHECK contains a subquery, aggregate, window function or set-returning
expression.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Use a scalar condition over the target row. CHECK cannot query other rows or
produce multiple rows. Static analysis does not execute the condition against
live database rows.

## Example

```sql
CREATE TABLE sample(value integer, CHECK((SELECT true)));
```

See [table CHECK constraints](../sql-analysis/statements/create-table.md#table-check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
