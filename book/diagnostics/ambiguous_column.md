# ambiguous_column

## Code and severity

Code: `ambiguous_column`. Severity: Error.

## When it is reported

An unqualified column name matches more than one visible source column.

## What analysis knows and does not know

The column name is retained, but analysis cannot select a unique binding or
infer its type from that reference. This differs from a missing column.

## How to address it

Qualify the intended column with a unique visible alias, such as `a.id`, or
remove the duplicate source.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer, value text)`.

```sql
SELECT id FROM docs_items a CROSS JOIN docs_items b;
```

Both sources expose `id`; `SELECT a.id FROM docs_items a CROSS JOIN docs_items
b` makes the intended source explicit.

## Related diagnostics

[column_not_found_in_source](column_not_found_in_source.md) reports no matching
column was found. [column_source_ambiguous](column_source_ambiguous.md) reports
the source qualifier itself is ambiguous.
