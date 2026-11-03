# query_invalid_compound_order

## Code and severity

Code: `query_invalid_compound_order`. Severity: Error.

## When it is reported

ORDER BY for a compound result does not identify an output column under the
selected dialect's binding rules.

## What analysis knows and does not know

Analysis knows the compound result columns, but cannot bind the ORDER BY item to
one of them. Ordinary SELECT expression ordering has different rules.

## How to address it

Refer to an output name or valid ordinal. If ordering by a new expression is
intended, wrap the compound query in an outer SELECT and order there.

## Example

PostgreSQL 18 builtin catalog. SQLite has its own compound-result matching
rules; this is a PostgreSQL example.

```sql
SELECT 1 UNION SELECT 2 ORDER BY 1 + 1;
```

`ORDER BY 1` identifies the first output column. `ORDER BY 1 + 1` is an
expression outside this compound output binding.

## Related diagnostics

[query_position_out_of_range](query_position_out_of_range.md) reports an ordinal
exists syntactically but exceeds the output list.
[query_reference_not_found](query_reference_not_found.md) reports another clause
reference has no binding.
