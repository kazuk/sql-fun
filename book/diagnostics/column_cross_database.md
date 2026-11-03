# column_cross_database

## Code and severity

Code: `column_cross_database`. Severity: Error.

## When it is reported

A database-qualified PostgreSQL column reference names a database different from
the explicitly configured `database-name`.

## What analysis knows and does not know

The requested qualification and configured database identity are available to
the resolver. Cross-database lookup is unsupported; no remote catalog lookup is
performed.

## How to address it

Use the selected database and a visible relation, or explicitly choose the
intended datasource/catalog. Do not change database identity merely to suppress
the diagnostic.

## Example

Illustrative PostgreSQL input: selected schema contains `docs_items(id integer)`
and analysis metadata explicitly sets `database-name = "sample"`. This code
requires that metadata to reach the resolver.

```sql
SELECT other.public.docs_items.id FROM docs_items;
```

`other` differs from `sample`. This is an analysis boundary, not evidence about
whether another database contains the table.

## Related diagnostics

[column_database_name_required](column_database_name_required.md) reports no
database identity is available.
[column_qualification_invalid](column_qualification_invalid.md) reports
qualification is invalid for the selected dialect.
