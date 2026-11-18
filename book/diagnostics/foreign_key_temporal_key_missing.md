# foreign_key_temporal_key_missing

## Code and severity

Code: `foreign_key_temporal_key_missing`. Severity: Error.

## When it is reported

A PostgreSQL 18 PERIOD foreign key has no matching WITHOUT OVERLAPS key on the
resolved parent table.

## What analysis knows and does not know

The diagnostic identifies the referenced table. With an omitted reference column
list, the intended key is the parent primary key.

## How to address it

Use a compatible temporal primary or unique key and mark the corresponding final
referenced column PERIOD when specifying the list.

## Example

```sql
CREATE TABLE child (a daterange, b daterange,
  FOREIGN KEY (a, PERIOD b) REFERENCES parent(a, PERIOD b));
```

## Related diagnostics

[foreign_key_referenced_primary_key_missing](foreign_key_referenced_primary_key_
missing.md)
