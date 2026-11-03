# ownership_argument_type_not_found

## Code and severity

Code: `ownership_argument_type_not_found`. Severity: Error.

## When it is reported

An ALTER OWNER target signature contains an input argument type that cannot be
resolved in the selected catalog. Output-only OUT/TABLE argument declarations
are not input signature types.

## What analysis knows and does not know

The requested object identity and unresolved type names are retained. A matching
routine or operator identity has not been established.

## How to address it

Correct or qualify the argument type names and load their declarations. Match
the actual input signature; do not invent a target type merely to make lookup
succeed.

## Example

PostgreSQL; no type named public.absent_type exists in the selected schema.

```sql
ALTER FUNCTION public.f(public.absent_type) OWNER TO CURRENT_USER;
```

The unresolved signature type is checked before deciding which routine matches.

## Related diagnostics

[ownership_target_not_found](ownership_target_not_found.md) concerns a resolved
signature with no matching object.
[ownership_target_ambiguous](ownership_target_ambiguous.md) concerns multiple
matches.
