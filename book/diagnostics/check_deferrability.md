# check_deferrability

Code: `check_deferrability`. Severity: Error.

## Meaning

A CHECK declares DEFERRABLE or INITIALLY DEFERRED.

Rejected CREATE/ALTER definitions do not modify the analysis catalog.

## How to fix

Remove the deferral option. PostgreSQL CHECK constraints cannot be deferred.
NOT VALID on ALTER ADD CHECK controls validation of existing rows; it does
not make CHECK deferrable.

## Example

```sql
CREATE TABLE sample(value integer, CHECK(value > 0) DEFERRABLE);
```

See [CHECK constraints](../sql-analysis/statements/create-table.md#check-constraints)
and [ALTER ADD CHECK](../sql-analysis/statements/alter-table.md#adding-table-check-constraints).
