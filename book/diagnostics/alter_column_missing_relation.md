# alter_column_missing_relation

## Code and severity

Code: `alter_column_missing_relation`. Severity: Error.

## When it is reported

An ALTER TABLE column-attribute operation cannot bind its target to a table,
unless an absent relation was explicitly allowed with IF EXISTS.

## What analysis knows and does not know

The requested table and column names are retained, but no table-column binding
was established. A missing binding does not prove absence from your live
database.

## How to address it

Check the table name, explicit schema, search path and selected schema
declarations. Use IF EXISTS only when skipping an absent target is intended.

## Example

PostgreSQL column-attribute syntax; the selected schema has no table named
absent.

```sql
ALTER TABLE public.absent ALTER COLUMN a SET NOT NULL;
```

This example concerns table lookup, before column-specific validation. SQLite
does not support this ALTER TABLE spelling.

## Related diagnostics

[alter_column_missing_column](alter_column_missing_column.md) means a table was
found but its column was not.
