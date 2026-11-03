# index_not_found

## Code and severity

Code: `index_not_found`. Severity: Error.

## When it is reported

ALTER TABLE ... CLUSTER ON names an index that cannot be found for the resolved
table's schema.

## What analysis knows and does not know

The requested index name is retained. The table has resolved; the index lookup
has not. This diagnostic does not by itself validate whether a found index
belongs to the table.

## How to address it

Check the index name and selected schema and ensure its definition is loaded
before the alteration.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer)` but no
`missing_index`.

```sql
ALTER TABLE docs_items CLUSTER ON missing_index;
```

Use an existing intended index in CLUSTER ON. This example is distinct from
altering a missing index directly.

## Related diagnostics

[index_invalid_alteration](index_invalid_alteration.md) reports ALTER INDEX
target or option validation. [relation_not_found](relation_not_found.md) reports
the table target did not resolve.
