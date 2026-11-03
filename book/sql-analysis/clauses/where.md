# WHERE

## Overview

WHERE expressions bind columns from the statement's visible sources and
enclosing query scopes. They participate in the same parameter constraints as
projections, CTEs and other clauses. PostgreSQL predicate parameter operands
impose boolean-compatible type requirements; a parameter used only in IS NULL
does not thereby acquire one input type. Analysis does not execute predicates or
determine selected rows.

```sql
SELECT item.id FROM items AS item WHERE item.id = $1;
```

For DML target/source visibility, see [UPDATE](../statements/update.md),
[DELETE](../statements/delete.md) and [MERGE](../statements/merge.md).
[Predicate expressions](../expressions/operators-and-predicates.md) own operator,
NULL, membership and pattern behavior. SELECT output aliases are not PostgreSQL
WHERE source bindings.

## Dialect and engine-specific behavior

SQLite predicate expressions retain SQLite semantics rather than PostgreSQL
boolean or implicit-cast constraints. SQLite expression and type-affinity analysis
remain incomplete. Grouping operations are checked for query-level placement;
this does not establish complete aggregate/window legality checking.

## Diagnostics

- [Name-resolution diagnostics](../expressions/names.md#diagnostics): missing or
  ambiguous predicate columns and sources.
- [parameter_constraint](../../diagnostics/parameter_constraint.md): repeated
  input uses have incompatible constraints.
- [query_invalid_grouping_placement](../../diagnostics/query_invalid_grouping_placement.md):
  GROUPING occurs in a forbidden WHERE scope, including applicable correlated
  occurrences.

The expression pages describe additional operation-specific diagnostics; success
here is not proof that every predicate type, database runtime constraint or
function behavior is valid.
