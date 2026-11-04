# call_argument_assigned_twice

## Code and severity

Code: `call_argument_assigned_twice`. Severity: Error.

## When it is reported

Every available candidate assigns the same formal input both positionally and
by name.

## What analysis knows and does not know

This is a function argument-binding failure, independent of the supplied value
types. The repeated formal input name is retained in the structured diagnostic.

The diagnostic is reported only when the full candidate set agrees on this
cause and formal name. A candidate that accepts the supplied argument placement
suppresses this diagnostic, even if its types subsequently fail. If candidates
fail for different reasons, analysis retains the general overload diagnostic.

## How to address it

Remove one assignment, or name the intended different input.

## Example

PostgreSQL 18 builtin catalog:

```sql
SELECT make_date(2024, year => 2025, month => 1, day => 1);
```

## Related diagnostics

[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports type or overload selection failures, or candidate failures without a
shared binding cause.
