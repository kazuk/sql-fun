# column_source_not_found

## Code and severity

Code: `column_source_not_found`. Severity: Error.

## When it is reported

A qualified column or whole-row reference has no matching visible source in its
scope.

## What analysis knows and does not know

The requested source qualification is retained. A matching catalog relation
alone is insufficient: it must also be visible in FROM under the applicable
alias.

## How to address it

Add the intended source to FROM or use its visible alias. An explicit alias can
hide the original relation name.

## Example

PostgreSQL 18 builtin schema; no project table is required to show this failure.

```sql
SELECT public.missing.id;
```

No source named `public.missing` is visible. Add and reference the intended
table explicitly.

## Related diagnostics

[undefined_alias](undefined_alias.md) reports a simple alias lookup failed.
[column_not_found_in_table](column_not_found_in_table.md) reports the source
exists but lacks the column.
