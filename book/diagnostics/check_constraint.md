# check_constraint

## Code and severity

Code: `check_constraint`. Severity: Error.

## When it is reported

A table CHECK expression cannot be converted to PostgreSQL boolean, contains
invalid boolean literal input, or uses a subquery, aggregate, window,
set-returning expression or bind parameter. A repeated PostgreSQL CHECK name or
invalid CHECK option placement also produces this diagnostic.

## What analysis knows and does not know

Analysis retains the expression and its bindings, options and rejected state.
Rejected CREATE/ALTER definitions do not modify the schema catalog. Analysis
does not execute the expression or check existing database rows.

## How to address it

Use a boolean condition over the target row, a distinct explicit constraint
name, and options allowed for that statement. CHECK is not deferrable.
CREATE TABLE treats new CHECK constraints as validated, even with NOT VALID.
SQLite uses its own truth-value
rules and does not require a PostgreSQL boolean expression.

## Example

PostgreSQL 17 or 18 builtin catalog; no project schema is required.

```sql
CREATE TABLE sample(value integer, CHECK(value));
```

Use a comparison such as `CHECK(value > 0)`.

## Related diagnostics

[column_not_found_in_table](column_not_found_in_table.md) reports missing row
columns; [relation_not_found](relation_not_found.md) reports missing targets.
