# drop_view_dependency

## Code and severity

Code: `drop_view_dependency`. Severity: Error.

## When it is reported

A view or materialized view depends on a view selected for removal under
`RESTRICT`.

## What analysis knows and does not know

The diagnostic names the dependent relation. All targets remain available.

## How to address it

Specify `CASCADE` if the dependent relations should also disappear. For ordinary
views, you can also list the related views in the same statement. If only a
column default depends on the view, remove the default before dropping its
dependency.

## Example

```sql
CREATE VIEW public.v AS SELECT 1 AS n;
CREATE VIEW public.w AS SELECT * FROM public.v;
DROP VIEW public.v;
```

## Related diagnostics

See [PostgreSQL view removal](../sql-analysis/statements.md#postgresql-view-
removal).
