# tablesample_relation

## Code and severity

Code: `tablesample_relation`. Severity: Error.

## When it is reported

TABLESAMPLE is applied to a resolved FROM source that is neither a table nor a
materialized view.

## What analysis knows and does not know

The source shape has been resolved enough to reject sampling. An ordinary view,
function source or other non-table source is not made sampleable by a valid
method.

## How to address it

Apply sampling to an appropriate base table or materialized view, or use a
different query design if sampling a derived result was intended.

## Example

PostgreSQL project schema contains ordinary view `public.v AS SELECT a FROM
public.t` and table public.t.

```sql
SELECT * FROM public.v TABLESAMPLE system(10);
```

An ordinary view is not a materialized view. The ordinary CLI may additionally
report missing sampling-method metadata.

## Related diagnostics

[tablesample_method_not_found](tablesample_method_not_found.md) concerns method
lookup rather than the source relation.
