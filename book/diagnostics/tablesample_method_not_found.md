# tablesample_method_not_found

## Code and severity

Code: `tablesample_method_not_found`. Severity: Error.

## When it is reported

A sampling-method lookup provider conclusively reports that the requested method
does not exist after its visibility rules.

## What analysis knows and does not know

The requested method name and absence classification are retained. This is
stronger than lacking metadata, but concerns the selected catalog rather than
all databases.

## How to address it

Check spelling, schema qualification and the explicitly selected catalog or
extension declarations.

## Example

Illustrative PostgreSQL analysis with table `public.t(a integer)` and a
sampling-method provider that establishes public.absent_sample is absent. The
ordinary CLI provider lacks method facts and reports metadata unavailable
instead.

```sql
SELECT * FROM public.t TABLESAMPLE public.absent_sample(10);
```

A provider must establish absence; omission of metadata is insufficient.

## Related diagnostics

[tablesample_metadata_unavailable](tablesample_metadata_unavailable.md) means
the facts are unavailable rather than the method absent.
