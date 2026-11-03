# ownership_target_not_found

## Code and severity

Code: `ownership_target_not_found`. Severity: Error.

## When it is reported

ALTER OWNER cannot find a catalog object matching the requested kind, visible
name and supplied input signature.

## What analysis knows and does not know

The requested typed identity is retained. A live database might contain an
object absent from the selected offline catalog.

## How to address it

Check the object name, kind, schema/search path, input signature and loaded
declarations. Ensure the schema represents the intended database version.

## Example

PostgreSQL; the selected schema has public.f(integer) and public.f(text), but no
boolean overload.

```sql
ALTER FUNCTION public.f(boolean) OWNER TO CURRENT_USER;
```

A symbolic owner keeps this example focused on target lookup.

## Related diagnostics

[ownership_target_ambiguous](ownership_target_ambiguous.md) concerns multiple
matches; [ownership_invalid_target_kind](ownership_invalid_target_kind.md)
concerns an incompatible object kind.
