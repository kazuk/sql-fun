# query_invalid_star_projection_placement

## Code and severity

Code: `query_invalid_star_projection_placement`. Severity: Error.

## When it is reported

Row-field expansion such as `(ROW(...)).*` appears where a scalar expression is
required rather than in a projection target.

## What analysis knows and does not know

The expansion location is known. This differs from passing a whole-row value
such as `t.*` to a function; expansion into multiple fields is not one scalar
argument.

## How to address it

Expand the row in a SELECT target list or pass the row value itself when the
called function accepts that type.

## Example

PostgreSQL 18 builtin catalog; `missing_function` need not exist because
expansion placement is checked independently.

```sql
SELECT missing_function((ROW(1)).*);
```

This example also reports `function_not_found`. A target-list expansion such as
`SELECT (ROW(1)).*` uses the supported location.

## Related diagnostics

[query_star_projection_requires_composite_type](query_star_projection_requires_composite_type.md)
reports the target value is not composite.
[query_scalar_value_required](query_scalar_value_required.md) reports a
non-scalar row/list reaches a scalar consumer.
