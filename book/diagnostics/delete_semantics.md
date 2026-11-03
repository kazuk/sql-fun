# delete_semantics

## Code and severity

Code: `delete_semantics`. Severity: Error.

## When it is reported

DELETE violates a checked target, predicate, cursor or result rule. Causes
include read-only targets, unsupported view operations, RETURNING scope, CTE
aliases, missing/non-updatable/mismatched cursors, unresolved procedural cursor
parameters, or SQLite-specific DELETE CTE/index-hint rules.

## What analysis knows and does not know

The rule category and explanation are retained; cursor failures can retain
normalized cursor and relation identities. A missing session cursor does not
establish whether a future runtime session would supply one.

## How to address it

Follow the specific explanation. Use a valid boolean predicate, writable target
and visible RETURNING names; establish an appropriate cursor when CURRENT OF is
intended. Check dialect-specific CTE and index-hint restrictions.

## Example

PostgreSQL project schema with `CREATE TABLE items (id integer);` already
loaded. The integer predicate violates PostgreSQL boolean requirements; SQLite
truth-value rules differ.

```sql
DELETE FROM items WHERE 1;
```

## Related diagnostics

[update_semantics](update_semantics.md) includes UPDATE cursor and predicate
rules; [view_missing_write_metadata](view_missing_write_metadata.md) concerns
missing view capabilities.
