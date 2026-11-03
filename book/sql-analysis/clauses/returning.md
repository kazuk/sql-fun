# RETURNING

## Overview

RETURNING describes DML result-column names, types and nullability without
executing the write. Without RETURNING, an analyzed INSERT, UPDATE, DELETE or
MERGE has no result columns. Expressions and nested subqueries share the
statement's parameter analysis. Explicit output aliases choose result names.

For INSERT/UPDATE/DELETE, an unqualified star expands only the destination.
PostgreSQL INSERT accepts a destination-qualified star. UPDATE/DELETE can also
reference applicable FROM/USING sources and their qualified stars. An explicit
target alias hides its original table name.
[MERGE](../statements/merge.md) owns its statement-specific source ordering,
merge_action(), action-dependent nullability and row-image scope.

A modifying [CTE](with.md) exposes RETURNING outputs, including explicit CTE
column aliases, rather than the modified table's complete column set. View
writability and RETURNING capability are separate decisions: a write being
permitted does not by itself establish that RETURNING is supported. See
[view analysis](../statements/create-view.md).

Executable-SQL [wildcard
expansion](../statements/select.md#wildcard-projection-expansion) is opt-in and
distinct from result-column expansion.

## Dialect and engine-specific behavior

SQLite INSERT/UPDATE/DELETE RETURNING uses the original table name rather than
the target alias and rejects qualified stars. SQLite does not support MERGE.
General SQLite expression/type-affinity limitations apply to its outputs.

Implicit PostgreSQL row images depend on the configured engine version and
statement. [DELETE](../statements/delete.md) owns its old/new image visibility
and nullability; [MERGE](../statements/merge.md) owns its action-dependent
images. Their PostgreSQL 18 support does not establish row-image support for
every DML statement.

PostgreSQL 18 RETURNING row-image alias syntax is recognized, but resolving
renamed images remains unimplemented
([#621](https://github.com/kazuk/sql-fun/issues/621)). Parser retention does not
establish support for RETURNING WITH (OLD AS ..., NEW AS ...). Ordinary DML and
MERGE retain their own analysis boundaries.

## Diagnostics

[insert_semantics](../../diagnostics/insert_semantics.md),
[update_semantics](../../diagnostics/update_semantics.md) and
[delete_semantics](../../diagnostics/delete_semantics.md) report invalid
statement-specific qualifiers or qualified stars, including SQLite restrictions.
[Name-resolution diagnostics](../expressions/names.md#diagnostics) apply to
missing or ambiguous output references. A star in an invalid expression position
or over a known scalar can produce
[query_invalid_star_projection_placement](../../diagnostics/query_invalid_star_projection_placement.md)
or
[query_star_projection_requires_composite_type](../../diagnostics/query_star_projection_requires_composite_type.md).

Output expression diagnostics and [parameter
finalization](../expressions/parameters.md#diagnostics) remain separate from
writable-target checks; known output metadata can survive an unresolved input
binding or another semantic problem.
