# table_function_not_found

## Code and severity

Code: `table_function_not_found`. Severity: Error.

## When it is reported

A FROM function source cannot be found in the selected catalog or resolved as a
table function.

## What analysis knows and does not know

The requested function name is retained. A missing source cannot establish its
output columns, so references to its outputs may remain unresolved.

## How to address it

Check the function name, qualification and loaded definitions. For an existing
record-returning function, satisfy its separate output declaration requirements.

## Example

PostgreSQL 18 builtin catalog; `missing_table_function` is absent. SQLite
table-function availability differs.

```sql
SELECT * FROM missing_table_function();
```

Load or reference the intended supported table function.

## Related diagnostics

[function_not_found](function_not_found.md) reports scalar call name lookup.
[parameter_type_mismatch_for_table_function](parameter_type_mismatch_for_table_function.md)
reports table-function candidates exist but arguments do not match.
