# cross_database_drop_view

## Code and severity

Code: `cross_database_drop_view`. Severity: Error.

## When it is reported

A PostgreSQL `DROP VIEW` contains a database qualifier that does not match the
analysis database.

## What analysis knows and does not know

The analyzer resolves targets in the selected database. The statement leaves the
catalog unchanged.

## How to address it

Select the intended database or qualify the view with its schema in the current
database.

## Example

```sql
DROP VIEW another_database.public.v;
```

## Related diagnostics

See [PostgreSQL view removal](../sql-analysis/statements.md#postgresql-view-
removal).
