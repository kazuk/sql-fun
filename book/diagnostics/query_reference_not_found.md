# query_reference_not_found

## Code and severity

Code: `query_reference_not_found`. Severity: Error.

## When it is reported

A clause-specific reference has no applicable definition: for example an OVER
window, ordering operator, lock source or star-projection source.

## What analysis knows and does not know

The reference text is retained. The code covers several clause namespaces; it
does not necessarily mean that a catalog table or ordinary column is missing.

## How to address it

Check the clause's name and visibility rules and declare or reference the
intended definition. Read the message to identify which clause failed.

## Example

PostgreSQL 18 builtin catalog; no tables are needed. Other dialect-specific
clause and dependency paths can use this code.

```sql
SELECT sum(1) OVER absent;
```

Declare `WINDOW absent AS ()` or use an existing declared window.

## Related diagnostics

[query_reference_ambiguous](query_reference_ambiguous.md) reports multiple
applicable definitions.
[column_not_found_in_source](column_not_found_in_source.md) reports an ordinary
column lookup failed.
