# query_position_out_of_range

## Code and severity

Code: `query_position_out_of_range`. Severity: Error.

## When it is reported

An ORDER BY or GROUP BY ordinal is outside the query's output list, including
zero or an ordinal greater than its width.

## What analysis knows and does not know

The requested ordinal is retained. Analysis knows the output positions; ordinals
are one-based.

## How to address it

Use an existing output position or a suitable output name/expression for that
clause.

## Example

PostgreSQL 18 builtin schema; no project tables are needed. Clause ordinal rules
are dialect-specific.

```sql
SELECT 1 ORDER BY 2;
```

The SELECT list has one output. `ORDER BY 1` refers to it; `ORDER BY 2` cannot.

## Related diagnostics

[query_invalid_compound_order](query_invalid_compound_order.md) reports a
compound ordering expression cannot bind to an output.
[query_reference_not_found](query_reference_not_found.md) reports a named clause
reference is missing.
