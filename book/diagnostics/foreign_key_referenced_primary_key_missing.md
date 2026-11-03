# foreign_key_referenced_primary_key_missing

## Code and severity

Code: `foreign_key_referenced_primary_key_missing`. Severity: Error.

## When it is reported

A PostgreSQL foreign key omits its referenced-column list, but the resolved
parent table has no primary-key columns to supply that list.

## What analysis knows and does not know

The referenced table name is retained. Omission requests the parent primary key,
not an arbitrary unique key or inferred column choice.

## How to address it

Give the parent an intended primary key, or explicitly name an eligible
referenced key column list. Verify compatible child/parent widths and types
separately.

## Example

PostgreSQL schema-loading example: load `CREATE TABLE parent (id integer);`
first, without a primary key. SQLite defers parent-key validity to DML
preparation; this PostgreSQL CREATE TABLE check does not establish the same
timing there.

```sql
CREATE TABLE child (parent_id integer,
  FOREIGN KEY (parent_id) REFERENCES parent);
```

## Related diagnostics

[relation_not_found](relation_not_found.md) cannot resolve the parent relation;
[column_not_found_in_table](column_not_found_in_table.md) concerns an explicitly
named missing parent column.
