# tablesample_arity

## Code and severity

Code: `tablesample_arity`. Severity: Error.

## When it is reported

A verified sampling method has a different number of declared sampling inputs
than the SQL supplies.

## What analysis knows and does not know

The method name and expected/supplied input counts are retained. Arity comes
from sampling inputs, not the handler function's dummy internal argument.

## How to address it

Pass the declared number of sampling inputs. Do not count the separate
REPEATABLE seed as a method argument.

## Example

Illustrative PostgreSQL analysis with a provider supplying verified TABLESAMPLE
method metadata and table `public.t(a integer)`. The ordinary CLI schema context
currently cannot supply those method facts, so it reports
`tablesample_metadata_unavailable` instead of reproducing this method-dependent
case.

```sql
SELECT * FROM public.t TABLESAMPLE system(1, 2);
```

With verified PostgreSQL system metadata, exactly one sampling argument is
expected.

## Related diagnostics

[tablesample_argument_type](tablesample_argument_type.md) concerns the types of
inputs after a matching argument count;
[tablesample_seed_type](tablesample_seed_type.md) concerns the separate seed.
