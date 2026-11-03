# query_invalid_grouping_placement

## Code and severity

Code: `query_invalid_grouping_placement`. Severity: Error.

## When it is reported

GROUPING occurs outside an allowed query expression context, or its argument has
invalid aggregate/window nesting.

## What analysis knows and does not know

Analysis retains the operation and its context. A matching GROUP BY key alone
does not make GROUPING legal in WHERE, JOIN conditions or another forbidden
location.

## How to address it

Use GROUPING in an allowed SELECT/HAVING expression and respect aggregate/window
nesting. Preserve the intended filter semantics when moving a condition.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer)`.

```sql
SELECT id FROM docs_items WHERE GROUPING(id) = 0 GROUP BY id;
```

WHERE is evaluated before grouping. If a grouped-result filter is intended, use
a legal HAVING expression instead.

## Related diagnostics

[query_grouping_argument_not_grouped](query_grouping_argument_not_grouped.md)
reports an argument is not a grouping key.
[query_too_many_grouping_arguments](query_too_many_grouping_arguments.md)
reports argument count exceeds the limit.
