# undefined_alias

## Code and severity

Code: `undefined_alias`. Severity: Error.

## When it is reported

A simple alias-qualified column reference cannot find that alias in the visible
FROM environment.

## What analysis knows and does not know

The alias is retained and no binding/type is established for the requested
column. A table definition in the catalog does not make an alias visible.

## How to address it

Correct the alias spelling or add the intended source under that alias. Respect
aliases that hide original table names.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer)`. Shared alias
lookup also serves dialect-specific sources.

```sql
SELECT missing.id FROM docs_items;
```

There is no FROM alias named `missing`. Use the visible source name or
explicitly alias it.

## Related diagnostics

[column_source_not_found](column_source_not_found.md) reports a qualified
relation or whole-row source lookup failed.
[column_not_found_in_table](column_not_found_in_table.md) reports the source
resolved but the column did not.
