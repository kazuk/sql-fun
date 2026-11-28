# unknown_database

Code: `unknown_database`. Severity: Error.

A PostgreSQL column, whole-row reference, or `DROP VIEW` target uses a database
qualifier absent from the analysis environment's local and related names.
Omitting both settings leaves every explicit database qualifier unknown.

For example, `SELECT sample.public.items.id FROM public.items` requires the
analysis environment to identify `sample` as the local database. If the
qualifier is unnecessary, use `public.items.id` or a visible alias.
PostgreSQL rejects direct cross-database references even when the analysis
environment knows the other database's name.

See [qualified column references](../sql-analysis/expressions/names.md).
