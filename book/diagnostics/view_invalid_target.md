# view_invalid_target

## Code and severity

Code: `view_invalid_target`. Severity: Error.

## When it is reported

A view-specific alteration cannot find a view of the requested
ordinary/materialized kind. An unqualified non-temporary CREATE VIEW also
reports this when no default schema is available.

## What analysis knows and does not know

The requested identity is retained, but the required view binding was not
established. A same-named table or the wrong kind of view is insufficient.

## How to address it

Check the target name, view kind, selected declarations and explicit
schema/search path. Qualify CREATE VIEW or supply an explicit default schema
where needed.

## Example

PostgreSQL analysis; no view named public.absent exists in the selected schema.

```sql
ALTER VIEW public.absent ALTER COLUMN a SET DEFAULT 1;
```

This is a target-binding failure, before checking which column receives a
default.

## Related diagnostics

[view_unknown_default_column](view_unknown_default_column.md) concerns a valid
view with an absent column. [view_not_found](view_not_found.md) is a separate
view-lookup diagnostic.
