# relation_not_found

## Code and severity

Code: `relation_not_found`. Severity: Error.

## When it is reported

A relation required by a FROM source, DML target, foreign key or alteration
cannot be resolved in the selected schema/search path.

## What analysis knows and does not know

The requested relation name is retained, but no relation binding exists. The
selected offline schema can differ from the live database.

## How to address it

Check spelling, qualification and the chosen schema. Load the intended relation
definition and use its visible alias in the query.

## Example

PostgreSQL 18 builtin catalog; `missing_relation` is absent. Shared relation
resolution also serves SQLite paths.

```sql
SELECT * FROM missing_relation;
```

Add the intended table/view to the selected schema rather than assuming it is
discoverable from a live database.

## Related diagnostics

[table_not_found](table_not_found.md) reports a table-specific catalog lookup
failed. [column_not_found_in_table](column_not_found_in_table.md) reports the
table exists but a column is missing.
