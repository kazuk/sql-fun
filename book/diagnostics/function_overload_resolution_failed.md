# function_overload_resolution_failed

## Code and severity

Code: `function_overload_resolution_failed`. Severity: Error.

## When it is reported

A scalar function name resolves, but its available overloads do not accept the
supplied argument bindings.

## What analysis knows and does not know

The function name and ordered known/unknown argument facts are retained. No
applicable scalar overload is selected.

## How to address it

Check the intended signature, argument count and types. Disambiguate inputs with
supported SQL types where needed; an arbitrary hint is not a valid fix for a
wrong signature.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT length(1, 2);
```

## Related diagnostics

[function_not_found](function_not_found.md) has no named definition;
[parameter_type_mismatch_for_table_function](parameter_type_mismatch_for_table_function.md)
concerns FROM functions.
