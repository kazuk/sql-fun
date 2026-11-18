# query_invalid_recursive_cte

## Code and severity

Code: `query_invalid_recursive_cte`. Severity: Error.

## When it is reported

A recursive `WITH` has duplicate names, a cycle involving multiple CTEs, or a
self-reference in an invalid position. In PostgreSQL, the diagnostic also
covers recursive result types that differ from their anchor types. An alias
list with more names than anchor columns is invalid.

## What analysis knows and does not know

Analysis uses the CTE's lexical scope and checks its query shape and the
types of its result columns. The [WITH reference](../sql-analysis/clauses/with.md)
describes the remaining type modifier, collation, and dialect limitations.

## How to address it

Give each CTE a distinct name and remove circular dependencies between CTEs.
Use `UNION` or `UNION ALL` with a nonrecursive anchor followed by a recursive
term. Keep the self-reference in the recursive term. For PostgreSQL, cast
the anchor to the intended result type when a recursive expression changes
that type.

## Example

On PostgreSQL 17 or 18 with the builtin catalog, the recursive expression
returns `bigint` while the anchor returns `integer`. Cast the anchor's `1` to
`bigint` to align the types.

```sql
WITH RECURSIVE t(n) AS (
    SELECT 1
    UNION ALL
    SELECT n::bigint FROM t WHERE false
)
SELECT n FROM t;
```

## Related diagnostics

- [query_set_column_count](query_set_column_count.md) reports different
  numbers of output columns in the two arms.
- [query_set_column_type](query_set_column_type.md) reports when the two
  branches lack a legal common type.
