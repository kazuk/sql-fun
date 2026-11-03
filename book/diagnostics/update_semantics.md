# update_semantics

## Code and severity

Code: `update_semantics`. Severity: Error.

## When it is reported

UPDATE violates a checked target, assignment, predicate, cursor or result rule.
Causes include overlapping targets, invalid DEFAULT subfield use,
generated/identity assignments, read-only views, conversion failures, RETURNING
scope, CTE aliases, or SQLite index hints.

## What analysis knows and does not know

The rule category and explanation are retained. Successful target-name
resolution does not establish that its requested assignments or predicate are
legal.

## How to address it

Correct the reported assignment or predicate, remove overlapping writes, and
respect generated/identity and view-write restrictions. Check cursor and
RETURNING scope independently where named.

## Example

PostgreSQL project schema with `CREATE TABLE items (id integer);` already
loaded. Boolean cannot be assigned to the integer destination through the
required assignment conversion. SQLite type and index-hint rules differ.

```sql
UPDATE items SET id = true;
```

## Related diagnostics

[insert_semantics](insert_semantics.md) concerns INSERT source/destination
rules; [delete_semantics](delete_semantics.md) concerns DELETE predicates,
cursors and result rules.
