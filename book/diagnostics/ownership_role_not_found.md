# ownership_role_not_found

## Code and severity

Code: `ownership_role_not_found`. Severity: Error.

## When it is reported

ALTER OWNER specifies a named role that is absent from the selected role
metadata.

## What analysis knows and does not know

The requested role name is retained. The target object lookup is independent and
can report another diagnostic. CURRENT_USER, CURRENT_ROLE and SESSION_USER use
separate symbolic owner forms.

## How to address it

Check the role spelling and selected role metadata. Load the intended role
declaration or deliberately choose the appropriate symbolic owner.

## Example

PostgreSQL; the project schema contains public.t and no role named absent_owner
is available in the analysis catalog.

```sql
ALTER TABLE public.t OWNER TO absent_owner;
```

The diagnostic concerns the proposed owner, not whether public.t exists or
whether a live connection has privileges.

## Related diagnostics

[ownership_target_not_found](ownership_target_not_found.md) concerns the object
whose ownership is being changed.
