# duplicate_alias

## Code and severity

Code: `duplicate_alias`. Severity: Error.

## When it is reported

Multiple visible query sources declare the same alias. The same code is also
used for a duplicate stored query-definition name during document merging.

## What analysis knows and does not know

The repeated alias or definition name is retained. Duplicate declaration is
distinct from a later ambiguous column use; an unused duplicate can still be
reported.

## How to address it

Give sources distinct aliases and update references. For merged query documents,
choose distinct query-definition names.

## Example

PostgreSQL 18 builtin schema; no project tables are needed. Query-document
merging is a separate non-SQL producer.

```sql
SELECT 1 FROM (SELECT 1) AS t CROSS JOIN (SELECT 2) AS t;
```

Rename one `t`, for example to `u`.

## Related diagnostics

[column_source_ambiguous](column_source_ambiguous.md) reports a use cannot
choose a source. [ambiguous_column](ambiguous_column.md) reports an unqualified
column cannot choose a binding.
