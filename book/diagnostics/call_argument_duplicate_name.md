# call_argument_duplicate_name

## Code and severity

Code: `call_argument_duplicate_name`. Severity: Error.

## When it is reported

A function call supplies the same named argument more than once after SQL
identifier normalization.

## What analysis knows and does not know

The duplicated argument name and source span, when available, are retained.
Signature selection may fail independently; this diagnostic is about call
syntax, not the chosen overload.

## How to address it

Supply each named argument once. Check quoted identifier spelling if two names
were intended to be different.

## Example

PostgreSQL 18 builtin catalog; named-argument notation is PostgreSQL syntax.

```sql
SELECT make_date(year => 2024, year => 2025, month => 1, day => 1);
```

Remove the second `year` argument; keep one year, one month and one day. An
overload diagnostic may accompany the duplicate-name diagnostic.

## Related diagnostics

[call_argument_positional_after_named](call_argument_positional_after_named.md)
reports argument ordering rather than repeated names.
[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports no applicable signature.
