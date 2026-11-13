# row_security_requires_table

## Code and severity

Code: `row_security_requires_table`. Severity: Error.

## When it is reported

A PostgreSQL row-security action resolves to a table, but the statement uses a
relation kind other than `ALTER TABLE`, such as `ALTER VIEW`, `ALTER SEQUENCE`
or `ALTER INDEX`.

## What analysis knows and does not know

The target table is resolved, but the requested alteration is inapplicable.
Analysis retains that outcome and leaves the table's row-security flags unchanged.
This diagnostic does not evaluate row policies or runtime permissions.

## How to address it

Use `ALTER TABLE` if the intended target is a table whose row-security flags you
want to change. Otherwise, use an operation supported for the intended relation
kind.

## Example

PostgreSQL; the selected project schema contains `CREATE TABLE public.t(a integer)`.

```sql
ALTER VIEW public.t ENABLE ROW LEVEL SECURITY;
```

The target is a table, so `ALTER VIEW` cannot change its row-security state.
For the intended table operation, use `ALTER TABLE public.t ENABLE ROW LEVEL SECURITY`.

## Related diagnostics

[table_not_found](table_not_found.md) reports a missing or unsuitable table target.
See [ALTER TABLE row-level security](../sql-analysis/statements/alter-table.md#postgresql-row-level-security)
for the supported operations.
