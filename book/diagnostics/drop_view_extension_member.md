# drop_view_extension_member

## Code and severity

Code: `drop_view_extension_member`. Severity: Error.

## When it is reported

A PostgreSQL `DROP VIEW` explicitly targets a member view outside its owning
extension’s installation or update script.

## What analysis knows and does not know

The selected view is still required by its extension. The drop leaves the
catalog unchanged.

## How to address it

Use the extension’s documented update procedure.

## Example

```sql
DROP VIEW public.pg_stat_statements;
```

## Related diagnostics

See [PostgreSQL view removal](../sql-analysis/statements.md#postgresql-view-
removal).
