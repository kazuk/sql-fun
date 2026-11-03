# column_not_found_in_cte

## Code and severity

Code: `column_not_found_in_cte`. Severity: Error.

## When it is reported

A reference binds to a CTE source, but the requested column is not among its
visible output columns.

## What analysis knows and does not know

The CTE and requested column names are retained. A CTE exists, but this
particular output binding and its type remain unresolved.

## How to address it

Use the CTE output name or expose the intended column in its SELECT list. Check
any explicit CTE column alias list.

## Example

PostgreSQL 18 builtin schema; no project tables are needed.

```sql
WITH c AS (SELECT 1 AS id) SELECT c.missing FROM c;
```

The CTE exposes `id`; use `c.id` or change the CTE output deliberately.

## Related diagnostics

[column_not_found_in_sub_query](column_not_found_in_sub_query.md) reports an
ordinary derived-table source.
[column_not_found_in_source](column_not_found_in_source.md) reports no matching
visible column.
