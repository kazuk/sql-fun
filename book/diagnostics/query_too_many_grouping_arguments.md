# query_too_many_grouping_arguments

## Code and severity

Code: `query_too_many_grouping_arguments`. Severity: Error.

## When it is reported

A GROUPING operation contains more than 31 arguments.

## What analysis knows and does not know

The argument count is known. Other placement or matching errors may also be
present; reducing the count alone does not establish a valid grouping
expression.

## How to address it

Use at most 31 grouping arguments per operation and ensure each matches a
grouping expression of the associated query.

## Example

PostgreSQL 18 builtin catalog; this SQL contains 32 arguments and no project
tables.

```sql
SELECT GROUPING(
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
);
```

The example also lacks matching grouping expressions. The limit applies even
when all 32 arguments would otherwise match.

## Related diagnostics

[query_grouping_argument_not_grouped](query_grouping_argument_not_grouped.md)
reports argument/key mismatch.
[query_invalid_grouping_placement](query_invalid_grouping_placement.md) reports
forbidden expression context.
