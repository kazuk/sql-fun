# insert_semantics

## Code and severity

Code: `insert_semantics`. Severity: Error.

## When it is reported

INSERT violates a checked source, destination or conflict rule. Causes include
source/destination width, repeated targets, read-only views, generated/identity
assignments, assignment conversions, RETURNING scope, conflict
arbiters/predicates, or CTE aliases.

## What analysis knows and does not know

The violated rule category and its explanation are retained. A target can
resolve while the requested insert remains inapplicable. Missing catalog write
metadata differs from proof that the database rejects the operation.

## How to address it

Follow the specific explanation: align columns and source values, respect
generated/identity rules, use a writable target and supported assignment types,
and select valid conflict targets. PostgreSQL and SQLite have different UPSERT
and RETURNING rules.

## Example

PostgreSQL project schema with `CREATE TABLE items (id integer);` already
loaded. This example has two source expressions for one destination column.

```sql
INSERT INTO items (id) VALUES (1, 2);
```

## Related diagnostics

[update_semantics](update_semantics.md) concerns UPDATE assignment rules;
[view_missing_write_metadata](view_missing_write_metadata.md) concerns
unavailable view-write facts.
