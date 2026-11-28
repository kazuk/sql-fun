# cross_database_drop_view

## Code and severity

Code: `cross_database_drop_view`. Severity: Error.

## When it is reported

A PostgreSQL `DROP VIEW` contains a qualifier that names a configured related
database. PostgreSQL disallows this cross-database reference.

## What analysis knows and does not know

The analyzer resolves targets in the selected database. The statement leaves the
catalog unchanged.

## How to address it

Select the intended database or qualify the view with its schema in the current
database.

## Example

Assume the analysis environment lists `another_database` as a related name.

```sql
DROP VIEW another_database.public.v;
```

## Related diagnostics

See [PostgreSQL view removal](../sql-analysis/statements.md#postgresql-view-
removal).
