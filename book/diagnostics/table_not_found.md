# table_not_found

## Code and severity

Code: `table_not_found`. Severity: Error.

## When it is reported

An operation requires a table-specific catalog definition and cannot resolve it,
for example the table named by sequence OWNED BY.

## What analysis knows and does not know

The requested table identity is retained. A generic relation or view is not
necessarily a valid substitute for a table in that operation.

## How to address it

Check the schema and name and load the intended table definition before the
operation.

## Example

PostgreSQL 18; selected schema contains `CREATE SEQUENCE docs_seq` but no
`missing_table`.

```sql
ALTER SEQUENCE docs_seq OWNED BY missing_table.id;
```

The sequence exists, but its requested owning table does not. This does not
establish whether a column of a real table would be valid for ownership.

## Related diagnostics

[relation_not_found](relation_not_found.md) reports a generic relation lookup
failed. [sequence_not_found](sequence_not_found.md) reports the sequence itself
is missing.
