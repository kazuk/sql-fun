# tablesample_metadata_unavailable

## Code and severity

Code: `tablesample_metadata_unavailable`. Severity: Error.

## When it is reported

The selected analysis context cannot establish the requested TABLESAMPLE method
facts. The ordinary CLI schema context currently uses this unavailable-metadata
outcome.

## What analysis knows and does not know

The requested method name is known. Absence, handler validity, accepted input
types and REPEATABLE support have not been established. This is not evidence
that PostgreSQL lacks or rejects the method.

## How to address it

Check whether your analysis provider supplies verified sampling-method metadata.
If it does not, retain this as an analysis limitation when reporting the query;
changing a valid method name cannot supply the missing facts.

## Example

PostgreSQL builtin catalog plus project table `public.t(a integer)`; the
ordinary CLI schema context lacks verified sampling-method metadata.

```sql
SELECT * FROM public.t TABLESAMPLE system(10);
```

This can be valid PostgreSQL SQL while remaining incomplete in offline analysis.

## Related diagnostics

[tablesample_method_not_found](tablesample_method_not_found.md) requires
conclusive absence.
[tablesample_invalid_handler](tablesample_invalid_handler.md) requires an
identified but unsuitable handler.
