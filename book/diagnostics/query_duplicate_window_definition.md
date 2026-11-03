# query_duplicate_window_definition

## Code and severity

Code: `query_duplicate_window_definition`. Severity: Error.

## When it is reported

A PostgreSQL WINDOW clause declares the same window name more than once.

## What analysis knows and does not know

The duplicate name is retained. A later OVER reference can additionally be
ambiguous; this diagnostic concerns the declarations themselves.

## How to address it

Assign unique names or combine identical window declarations into one, updating
OVER references accordingly.

## Example

PostgreSQL 18 builtin schema; no tables are needed. The current
duplicate-definition check is PostgreSQL-specific.

```sql
SELECT 1 WINDOW w AS (), w AS ();
```

Remove one declaration of `w` if only one window is intended.

## Related diagnostics

[query_reference_ambiguous](query_reference_ambiguous.md) reports a window
reference has multiple definitions.
[query_reference_not_found](query_reference_not_found.md) reports a window
reference has no definition.
