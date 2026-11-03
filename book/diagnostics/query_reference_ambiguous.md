# query_reference_ambiguous

## Code and severity

Code: `query_reference_ambiguous`. Severity: Error.

## When it is reported

A query-clause reference identifies more than one applicable definition, such as
an OVER name with duplicate WINDOW declarations.

## What analysis knows and does not know

The reference name is retained, but there is no unique completed binding.
Ambiguity is not the same as absence.

## How to address it

Remove or rename duplicate definitions and qualify references where the clause
supports qualification.

## Example

PostgreSQL 18 builtin catalog; no tables are needed. SQLite rename/dependency
analysis also uses this code for ambiguous preserved references.

```sql
SELECT sum(1) OVER w WINDOW w AS (), w AS ();
```

Both WINDOW declarations are named `w`. This also reports
`query_duplicate_window_definition`.

## Related diagnostics

[query_reference_not_found](query_reference_not_found.md) reports no applicable
definition.
[query_duplicate_window_definition](query_duplicate_window_definition.md)
reports duplicate declarations, even without a use.
