# tablesample_argument_type

## Code and severity

Code: `tablesample_argument_type`. Severity: Error.

## When it is reported

A known TABLESAMPLE argument type is neither the verified target type nor
assignment-coercible to it.

## What analysis knows and does not know

The expected and supplied types are retained. An unresolved source type does not
by itself trigger this incompatibility diagnostic.

## How to address it

Supply an expression compatible with the declared sampling-input type. Resolve
separate name or type-inference diagnostics first; do not choose an arbitrary
cast merely to silence the report.

## Example

Illustrative PostgreSQL analysis with a provider supplying verified TABLESAMPLE
method metadata and table `public.t(a integer)`. The ordinary CLI schema context
currently cannot supply those method facts, so it reports
`tablesample_metadata_unavailable` instead of reproducing this method-dependent
case.

```sql
SELECT * FROM public.t TABLESAMPLE system(true);
```

Verified system metadata expects a float4-compatible percentage, not boolean.
This does not validate the runtime percentage range.

## Related diagnostics

[tablesample_arity](tablesample_arity.md) concerns count;
[tablesample_seed_type](tablesample_seed_type.md) concerns REPEATABLE.
