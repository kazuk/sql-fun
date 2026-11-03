# column_database_name_required

## Code and severity

Code: `column_database_name_required`. Severity: Error.

## When it is reported

A PostgreSQL column reference includes a database qualifier, but analysis has no
explicitly configured database identity to compare it with.

## What analysis knows and does not know

The requested qualified reference is retained. The analyzer cannot determine
whether its database component names the current database and does not infer a
database identity.

## How to address it

If the database qualifier is unnecessary, use a visible alias or
schema-qualified relation. Otherwise supply the intended `database-name`
metadata explicitly.

## Example

PostgreSQL 18; no `database-name` is supplied; selected schema contains
`docs_items(id integer)`.

```sql
SELECT sample.public.docs_items.id FROM docs_items;
```

For example, `SELECT docs_items.id FROM docs_items` avoids a database-qualified
reference.

## Related diagnostics

[column_cross_database](column_cross_database.md) reports a known configured
database differs from the requested one.
[column_source_not_found](column_source_not_found.md) reports no visible source
matches.
