# ownership_target_ambiguous

## Code and severity

Code: `ownership_target_ambiguous`. Severity: Error.

## When it is reported

An ALTER OWNER target matches more than one eligible identity after visibility
rules, commonly because an overloaded routine was named without its input
signature.

## What analysis knows and does not know

The ambiguous candidate identities are retained. No single ownership target was
selected automatically.

## How to address it

Provide the intended routine input signature, with schema qualification when
needed. Distinguish same-named objects explicitly rather than relying on catalog
enumeration order.

## Example

PostgreSQL; the schema contains both `public.f(integer)` and `public.f(text)` as
functions.

```sql
ALTER FUNCTION public.f OWNER TO CURRENT_USER;
```

To select the integer overload, use ALTER FUNCTION public.f(integer) OWNER TO
CURRENT_USER.

## Related diagnostics

[ownership_target_not_found](ownership_target_not_found.md) means no matching
definition;
[ownership_argument_type_not_found](ownership_argument_type_not_found.md) means
a signature type was unresolved.
