# column_not_found_in_sub_query

## Code and severity

Code: `column_not_found_in_sub_query`. Severity: Error.

## When it is reported

A qualified reference reaches a derived SELECT source, but its requested output
column is absent.

## What analysis knows and does not know

The derived-table alias and requested column are retained. Analysis has
identified the source but cannot bind that output name.

## How to address it

Use a name exposed by the subquery, add the intended expression to its
projection, or correct its column alias list.

## Example

PostgreSQL 18 builtin schema; no project tables are needed.

```sql
SELECT s.missing FROM (SELECT 1 AS id) AS s;
```

The subquery exposes `id`, so `s.id` is available while `s.missing` is not.

## Related diagnostics

[sub_query_not_have_column_for_derived_column_name](sub_query_not_have_column_for_derived_column_name.md)
reports an alias position exceeds the subquery width.
[column_not_found_in_cte](column_not_found_in_cte.md) reports a CTE output is
missing.
