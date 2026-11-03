# tablesample_repeatable

## Code and severity

Code: `tablesample_repeatable`. Severity: Error.

## When it is reported

An explicit REPEATABLE clause is used with a verified method whose metadata
states it does not support repeatable sampling.

## What analysis knows and does not know

The method name and unsupported capability are known. This is not a seed-type
error or a conclusion drawn merely from missing metadata.

## How to address it

Remove REPEATABLE if nondeterministic sampling is intended, or explicitly select
a method known to support it.

## Example

Illustrative PostgreSQL provider case: table public.t exists and custom_sample
is verified to accept one integer input but not REPEATABLE. Ordinary CLI
analysis cannot supply these method facts.

```sql
SELECT * FROM public.t TABLESAMPLE custom_sample(10) REPEATABLE(1);
```

A numeric seed does not grant the method repeatability support.

## Related diagnostics

[tablesample_seed_type](tablesample_seed_type.md) concerns seed coercion;
[tablesample_metadata_unavailable](tablesample_metadata_unavailable.md) concerns
unknown method facts.
