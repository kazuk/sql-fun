# alter_column_primary_key

## Code and severity

Code: `alter_column_primary_key`. Severity: Error.

## When it is reported

DROP NOT NULL targets a column belonging to an inline or table-level primary
key.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved.

## How to address it

Keep primary-key columns non-null. If the key design must change, change the
constraint deliberately before attempting to make the column nullable.

## Example

PostgreSQL; the project schema contains `public.key_table(id integer PRIMARY
KEY)`.

```sql
ALTER TABLE public.key_table ALTER COLUMN id DROP NOT NULL;
```

The primary-key constraint prevents this nullability change.

## Related diagnostics

[alter_column_identity](alter_column_identity.md) covers the independent
requirement that identity columns remain non-null.
