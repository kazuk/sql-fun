# function_overload_resolution_failed

## Code and severity

Code: `function_overload_resolution_failed`. Severity: Error.

## When it is reported

A scalar function name resolves, but its available overloads do not accept the
supplied argument bindings, or more than one applicable signature remains.
For named calls, a binding failure shared by all candidates instead uses
[call_argument_unknown_name](call_argument_unknown_name.md),
[call_argument_missing_required](call_argument_missing_required.md), or
[call_argument_assigned_twice](call_argument_assigned_twice.md).
If candidates fail for different reasons, this general diagnostic remains.

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
