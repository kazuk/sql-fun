# alter_column_not_null_conflict

## Code and severity

Code: `alter_column_not_null_conflict`. Severity: Error.

## When it is reported

On PostgreSQL 18 and later, an ALTER TABLE statement requests a conflicting
named NOT NULL constraint on a primary-key column that already has one.
A different constraint name or NO INHERIT setting can cause this diagnostic.

## What analysis knows and does not know

Analysis uses the existing constraint retained in the selected project schema.
A conflicting declaration leaves the entire ALTER TABLE statement unapplied.
For PostgreSQL 18, analysis follows the behavior introduced in 18.2.

## How to address it

Use the existing constraint name and settings if the statement must refer to
that constraint. If the primary key already guarantees non-nullability, remove
the redundant addition from the migration.

## Example

The project schema contains `CREATE TABLE key_table (id integer PRIMARY KEY)`.

```sql
ALTER TABLE key_table ADD CONSTRAINT id_required NOT NULL id NOT VALID;
```

When PostgreSQL names the existing constraint `key_table_id_not_null`, the new
name `id_required` conflicts with it.

## Related diagnostics

[alter_column_primary_key](alter_column_primary_key.md) covers dropping the
non-nullability required by a primary key.
