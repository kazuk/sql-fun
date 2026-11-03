# column_already_exists

## Code and severity

Code: `column_already_exists`. Severity: Error.

## When it is reported

A column addition or rename would reuse an existing column name in the target
table.

## What analysis knows and does not know

The table and conflicting column name are retained. Rename checks use
dialect-specific identifier comparison; SQLite can permit a rename to the same
bound column.

## How to address it

Choose a distinct destination name or remove an unintended duplicate
declaration. Check the schema being analyzed before changing an existing column.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer, value text)`.
SQLite rename checks additionally use its identifier rules.

```sql
ALTER TABLE docs_items RENAME COLUMN id TO value;
```

Renaming `id` to `value` conflicts with the existing `value` column.

## Related diagnostics

[column_not_found_in_table](column_not_found_in_table.md) reports the requested
source column is missing. [conflicting_creation](conflicting_creation.md)
reports a broader catalog identity conflict.
