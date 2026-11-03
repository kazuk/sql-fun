# view_missing_write_metadata

## Code and severity

Code: `view_missing_write_metadata`. Severity: Error.

## When it is reported

Analysis of a view write cannot obtain a required semantic view definition or
bound view identity from the explicitly loaded catalog.

## What analysis knows and does not know

The view name whose definition is missing is retained. Missing facts do not
establish that PostgreSQL rejects writes to that view or that the view is
read-only.

## How to address it

Reload complete schema declarations for the selected view and its dependencies.
If the selected analysis provider cannot supply the definition, report that
limitation rather than assuming a write capability.

## Example

Illustrative PostgreSQL metadata-dependent case: public.v is known as a view,
but its required semantic definition is absent from the selected provider. A
complete CREATE VIEW declaration alone should not trigger this condition.

```sql
INSERT INTO public.v(a) VALUES (1);
```

The missing provider state is essential; this is not a standalone SQL reproducer
against a complete schema.

## Related diagnostics

[insert_semantics](insert_semantics.md), [update_semantics](update_semantics.md)
and [delete_semantics](delete_semantics.md) cover established write-rule
violations rather than a missing required definition.
