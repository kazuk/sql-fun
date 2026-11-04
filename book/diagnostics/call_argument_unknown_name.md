# call_argument_unknown_name

## Code and severity

Code: `call_argument_unknown_name`. Severity: Error.

## When it is reported

A supplied name is unknown to every available candidate.

## What analysis knows and does not know

This is a function argument-binding failure, independent of the supplied value
types. The supplied argument name is retained in the structured diagnostic.

The diagnostic is reported only when the full candidate set agrees on this
cause and formal name. A candidate that accepts the supplied argument placement
suppresses this diagnostic, even if its types subsequently fail. If candidates
fail for different reasons, analysis retains the general overload diagnostic.

## How to address it

Use an input or INOUT argument name from the intended function declaration.
OUT-only names are not callable inputs.

## Example

PostgreSQL 18 builtin catalog:

```sql
SELECT make_date(unknown => 2024);
```

## Related diagnostics

[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports type or overload selection failures, or candidate failures without a
shared binding cause.
