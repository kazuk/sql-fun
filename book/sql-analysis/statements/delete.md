# DELETE

## Overview

PostgreSQL and SQLite DELETE analysis checks the target, WITH queries, predicate
and RETURNING columns. PostgreSQL USING sources, ONLY/inheritance and positioned
`WHERE CURRENT OF` are recognized. Analysis does not delete rows or remove
schema definitions. Without RETURNING, a DELETE has no result columns. Invalid
DELETE SQL produces inspectable analysis problems, including missing or
read-only destinations. Analyzer failures remain separate from SQL diagnostics.

[RETURNING](../clauses/returning.md) follows the shared target/source visibility
and dialect rules; PostgreSQL USING sources are visible. Explicit SQLite
INDEXED BY hints must name an index of the target.

Shared [WITH](../clauses/with.md) rules govern RETURNING outputs, explicit CTE
column aliases and top-level placement. SQLite permits WITH before DELETE,
but not DELETE inside a CTE body.

The configured engine version governs PostgreSQL RETURNING `old`/`new` names:
these implicit row references are available starting with PostgreSQL 18.
Explicit source aliases named `old` or `new` remain ordinary source bindings.
The old image retains target-column nullability; the new image is nullable for
DELETE. The shared [RETURNING support boundary](../clauses/returning.md)
distinguishes PostgreSQL 18 parser retention from unimplemented renamed-image
binding.

For view targets, permission to DELETE and support for RETURNING are independent
of INSERT/UPDATE support. Materialized views are read-only. The bundled
catalog still lacks the trigger/rule metadata needed to establish ordinary-view
DELETE capabilities, so its view-capability lookup remains unimplemented.
Production catalog support and integration tests are tracked in
[#589](https://github.com/kazuk/sql-fun/issues/589). Positioned DELETE uses
explicitly supplied current-session cursor facts; analysis does not check
runtime cursor position or lifetime. The general SQLite expression and
type-affinity [limitations](../expressions.md) also apply to DELETE expressions.

Shared [WITH](../clauses/with.md), [FROM](../clauses/from.md),
[WHERE](../clauses/where.md) and [RETURNING](../clauses/returning.md) semantics
apply to the supported paths.

## Dialect and engine-specific behavior

PostgreSQL and SQLite use dedicated DELETE analysis. Version-specific PostgreSQL
row-image rules and SQLite scope/index-hint distinctions are described above.
SQLite UPDATE/DELETE ORDER BY or LIMIT grammar remains unavailable in the
inherited parser; enabling SQLITE_ENABLE_UPDATE_DELETE_LIMIT in a database does
not change sql-fun's parser. See the [overview](../../sql-analysis.md) for the
remaining parser limits.

## Diagnostics

- [relation_not_found](../../diagnostics/relation_not_found.md): the destination
  relation cannot be resolved.
- [delete_semantics](../../diagnostics/delete_semantics.md): a checked target
  capability, RETURNING rule, predicate, index hint, cursor or DML CTE
  requirement fails.

Shared expression and name-resolution findings are described at their owning
[expression pages](../expressions.md). A parser or analyzer failure is distinct
from a structured semantic diagnostic.
