# sequence_not_found

## Code and severity

Code: `sequence_not_found`. Severity: Error.

## When it is reported

A sequence-specific alteration or catalog application cannot find the requested
sequence.

## What analysis knows and does not know

The requested sequence identity is retained. This differs from looking up a
table or view with the same spelling.

## How to address it

Check the selected schema, qualification and sequence definition. Ensure it is
loaded before the sequence operation.

## Example

PostgreSQL 18 builtin catalog; `missing_sequence` is absent. PostgreSQL
sequences are not SQLite AUTOINCREMENT columns.

```sql
ALTER SEQUENCE missing_sequence RESTART;
```

Choose the intended existing sequence for RESTART, or provide its schema
definition first.

## Related diagnostics

[table_not_found](table_not_found.md) reports a table required by sequence
ownership is missing.
[ownership_target_not_found](ownership_target_not_found.md) reports an ALTER
OWNER target cannot be resolved.
