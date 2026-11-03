# function_arg_implicit_cast_not_found

## Code and severity

Code: `function_arg_implicit_cast_not_found`. Severity: Error.

## When it is reported

After selecting a function overload, a known argument type cannot be implicitly
matched to the corresponding declared argument type.

## What analysis knows and does not know

The function, argument index, expected type definition and argument facts are
retained. This is a conversion failure for an already selected signature.

## How to address it

Check the selected signature and argument expression. Use a supported explicit
conversion only where it matches the intended input; verify custom declarations
and catalog conversion metadata.

## Example

Illustrative PostgreSQL input: `custom_function` and `arguments.value` must
resolve such that a selected signature still fails argument conversion. Ordinary
incompatible calls can instead stop earlier at overload selection. This is not a
standalone reproducer with the builtin catalog.

```sql
SELECT custom_function(value) FROM arguments;
```

## Related diagnostics

[function_overload_resolution_failed](function_overload_resolution_failed.md)
fails signature selection; [implicit_cast_not_found](implicit_cast_not_found.md)
concerns a general required implicit conversion.
