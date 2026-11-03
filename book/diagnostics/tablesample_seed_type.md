# tablesample_seed_type

## Code and severity

Code: `tablesample_seed_type`. Severity: Error.

## When it is reported

A known REPEATABLE seed type cannot be assignment-coerced to the required float8
target.

## What analysis knows and does not know

Expected and supplied types are retained. Seed typing is separate from the
method's support for repeatable sampling and from the runtime value of the seed.

## How to address it

Use a float8-compatible seed expression and check any independent
method-capability diagnostic.

## Example

PostgreSQL analysis with project table `public.t(a integer)`. The seed can be
checked even if method metadata is unavailable.

```sql
SELECT * FROM public.t TABLESAMPLE system(10) REPEATABLE(true);
```

A boolean seed fails this type check. Ordinary CLI analysis also reports
tablesample_metadata_unavailable for the method.

## Related diagnostics

[tablesample_repeatable](tablesample_repeatable.md) concerns whether the method
supports REPEATABLE; [tablesample_argument_type](tablesample_argument_type.md)
concerns sampling inputs.
