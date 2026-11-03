# call_argument_positional_after_named

## Code and severity

Code: `call_argument_positional_after_named`. Severity: Error.

## When it is reported

A positional function argument appears after a named argument.

## What analysis knows and does not know

The offending argument position and available source span are retained. Analysis
has not established that moving the argument alone will make an overload
applicable.

## How to address it

Put positional arguments first, followed by named arguments, or name all
supplied arguments.

## Example

PostgreSQL 18 builtin catalog; SQLite does not use this named-argument notation.

```sql
SELECT make_date(year => 2024, 1, 1);
```

For example, `SELECT make_date(2024, 1, 1)` supplies three positional arguments
in signature order.

## Related diagnostics

[call_argument_duplicate_name](call_argument_duplicate_name.md) reports repeated
names.
[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports signature applicability.
