# ownership_invalid_target_kind

## Code and severity

Code: `ownership_invalid_target_kind`. Severity: Error.

## When it is reported

An ALTER OWNER request names an unsupported ownership kind, a type/domain of an
incompatible kind, or a function/procedure identity that resolves only to the
other routine kind.

## What analysis knows and does not know

The requested typed identity is retained. This is different from conclusively
finding no definition under that identity.

## How to address it

Use the actual object kind and input signature. For a type/domain request, check
whether the retained definition is a domain, relation row type or array type.

## Example

PostgreSQL; the schema declares `public.f(integer)` as a function, not a
procedure.

```sql
ALTER PROCEDURE public.f(integer) OWNER TO CURRENT_USER;
```

The routine exists under a different kind. SQLite has no corresponding ownership
operation.

## Related diagnostics

[ownership_target_not_found](ownership_target_not_found.md) concerns no matching
object; [ownership_role_not_found](ownership_role_not_found.md) concerns the new
owner.
