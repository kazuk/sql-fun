# parameter_type_mismatch_for_table_function

## Code and severity

Code: `parameter_type_mismatch_for_table_function`. Severity: Error.

## When it is reported

A FROM function has definitions, but no overload accepts the supplied argument
bindings.

## What analysis knows and does not know

The function name and ordered argument type facts are retained. The analyzer
cannot bind a usable table-function source or establish its normal output
columns.

## How to address it

Check argument count and types against the intended table function. Supply an
array to an array-expanding function rather than an unrelated scalar.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. A secondary
query-reference diagnostic can follow because the source has no usable output
definition.

```sql
SELECT * FROM unnest(1::smallint);
```

## Related diagnostics

[table_function_not_found](table_function_not_found.md) has no named function;
[function_overload_resolution_failed](function_overload_resolution_failed.md)
concerns scalar function binding.
