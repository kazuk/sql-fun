# call_argument_missing_required

## Code and severity

Code: `call_argument_missing_required`. Severity: Error.

## When it is reported

Every available candidate requires the same omitted input.

## What analysis knows and does not know

This is a function argument-binding failure, independent of the supplied value
types. The formal name is retained in the structured diagnostic when available.
A missing required input may be unnamed.

The diagnostic is reported only when the full candidate set agrees on this
cause and formal name. A candidate that accepts the supplied argument placement
suppresses this diagnostic, even if its types subsequently fail. If candidates
fail for different reasons, analysis retains the general overload diagnostic.

## How to address it

Supply the required input. Only inputs with defaults may be omitted.

## Example

PostgreSQL 18 builtin catalog:

```sql
SELECT make_date(month => 1, day => 1);
```

## Related diagnostics

[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports type or overload selection failures, or candidate failures without a
shared binding cause.
