# query_grouping_argument_not_grouped

## Code and severity

Code: `query_grouping_argument_not_grouped`. Severity: Error.

## When it is reported

A GROUPING argument does not match a grouping expression belonging to the
associated query level.

## What analysis knows and does not know

The analyzer has identified a GROUPING operation and its owning query. It cannot
treat a different expression as an equivalent grouping key merely because it is
derivable from a key.

## How to address it

Use the actual GROUP BY expressions as GROUPING arguments and check correlated
query levels. Add a grouping expression only when required by the intended
query.

## Example

PostgreSQL 18 builtin schema; no project tables are needed. GROUPING support and
rules are PostgreSQL-specific here.

```sql
SELECT GROUPING(1);
```

There is no matching grouping key. Use an actual grouping expression of the
associated query, rather than introducing a different expression with a similar
value.

## Related diagnostics

[query_invalid_grouping_placement](query_invalid_grouping_placement.md) reports
GROUPING appears in a forbidden context.
[query_too_many_grouping_arguments](query_too_many_grouping_arguments.md)
reports more than 31 arguments.
