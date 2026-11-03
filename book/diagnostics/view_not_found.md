# view_not_found

## Code and severity

Code: `view_not_found`. Severity: Error.

## When it is reported

Catalog application of a previously analyzed view alteration cannot find the
requested view definition.

## What analysis knows and does not know

The requested view identity is retained. This is an application-stage lookup;
ordinary SQL analysis can reject a missing view earlier with
`view_invalid_target`.

## How to address it

Check that the alteration is applied to the intended schema context and that its
view definition is present. Reanalyze against the current context if the catalog
changed.

## Example

Illustrative PostgreSQL alteration of an absent view; no project tables are
required. A normal CLI analysis reports `view_invalid_target` first; this code
concerns applying an already analyzed alteration through the schema API.

```sql
ALTER VIEW missing_view ALTER COLUMN id SET DEFAULT 1;
```

The input shows the required view identity. It is not a standalone CLI
reproducer for this application-stage diagnostic.

## Related diagnostics

[view_invalid_target](view_invalid_target.md) reports SQL analysis rejects the
target or materialization kind.
[column_not_found_in_view](column_not_found_in_view.md) reports the view exists
but lacks an output column.
