# sub_query_not_have_column_for_derived_column_name

## Code and severity

Code: `sub_query_not_have_column_for_derived_column_name`. Severity: Error.

## When it is reported

A derived-table column alias refers to a position beyond the underlying
subquery's output width.

## What analysis knows and does not know

Analysis knows the requested alias position has no corresponding output. This
code does not retain extra operand identity or width facts.

## How to address it

Keep the derived alias list within the actual output width, or add the intended
output expression to the subquery.

## Example

PostgreSQL-style SQLAST example; no schema is needed. This producer is
source-verified, but current CLI transport cannot decode this diagnostic
payload, so this is not a successful CLI reproduction.

```sql
SELECT s.second FROM (SELECT 1) AS s(first, second);
```

The subquery has one output, while `second` names a second alias position. Use
`s.first` with one alias, or add a second output.

## Related diagnostics

[column_not_found_in_sub_query](column_not_found_in_sub_query.md) reports a
requested name is absent without an out-of-width alias position.
[query_set_column_count](query_set_column_count.md) reports set-operation widths
differ.
