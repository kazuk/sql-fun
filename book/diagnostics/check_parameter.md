# check_parameter

Code: `check_parameter`. Severity: Error.

## Meaning

A table CHECK contains a bind parameter.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Define the schema condition without application bind parameters. Use a
literal, target-row column or supported scalar expression instead.

## Example

```sql
CREATE TABLE sample(value integer, CHECK($1));
```

See [table CHECK constraints](../sql-analysis/statements/create-table.md#table-check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
