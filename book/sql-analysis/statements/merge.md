# MERGE

## Overview

PostgreSQL MERGE analysis retains its target, USING source, ON condition and
ordered WHEN actions. PostgreSQL 15 and 16 support MATCHED and NOT MATCHED
actions. PostgreSQL 17 adds NOT MATCHED BY SOURCE, RETURNING and MERGE inside
nonrecursive data-modifying CTEs. The configured engine version governs these
features; SQLite does not support MERGE.

MATCHED conditions and UPDATE values can reference both target and source. NOT
MATCHED (BY TARGET) can reference only the source; NOT MATCHED BY SOURCE can
reference only the target. These restrictions also apply to correlated
subqueries. USING subqueries cannot correlate to the target. An unconditional
WHEN makes later WHEN clauses of the same match kind unreachable. INSERT is
legal only for NOT MATCHED BY TARGET; UPDATE and DELETE require a target row. DO
NOTHING is legal for every match kind. WITH RECURSIVE is unsupported.

RETURNING belongs to the whole MERGE. It can reference target and source columns
and `merge_action()`, including inside RETURNING subqueries. Unqualified `*`
expands source columns before target columns; qualified stars expand the named
record. A source row can be absent when an action runs for NOT MATCHED BY
SOURCE. Starting with PostgreSQL 18, implicit `old` and `new` references expose
target row images. The old image is nullable when INSERT is possible, and the
new image is nullable when DELETE is possible. Explicit relation aliases named
`old` or `new` take precedence. Renamed images follow the shared
[RETURNING support boundary](../clauses/returning.md). The parser normalizes
plain `NOT MATCHED` and `NOT MATCHED BY TARGET` to the same syntax node, so
analysis cannot diagnose use of the explicit `BY TARGET` spelling against
PostgreSQL 15 or 16; that spelling requires PostgreSQL 17.

A MERGE CTE exposes only RETURNING columns, with any explicit CTE column
aliases. A MERGE CTE without RETURNING may be unreferenced but cannot be read as
a relation. Data-modifying CTEs must occur at the top level. Parameters across
ON, WHEN conditions, action values and RETURNING participate in one analysis.
The shared [expression](../expressions.md), subquery and catalog limitations also
apply to MERGE; analysis does not execute its actions.

API consumers supply view-target facts through `ViewMergeInfoRead`. PostgreSQL
17 and later permit automatically updatable views without INSTEAD OF triggers,
or views with an INSTEAD OF trigger for every action type named by the MERGE.
Views with rules and materialized views are rejected. Effective writable-column
metadata governs action assignments. These facts are independent of ordinary DML
capability flags. The bundled production catalogs lack this metadata; production
view-target support and integration tests are tracked in
[#609](https://github.com/kazuk/sql-fun/issues/609). Custom contexts can provide
it.

See the shared [WITH](../clauses/with.md), [FROM](../clauses/from.md) and
[RETURNING](../clauses/returning.md) pages for their common rules.

## Dialect and engine-specific behavior

MERGE is PostgreSQL-only. The configured PostgreSQL version controls the 15/16
versus 17+ features and PostgreSQL 18 row images described above. An available
parse tree or custom view-capability provider is not evidence that the
production catalog can complete that same path.

## Diagnostics

MERGE's implemented version gates can emit
[incompatible_engine_version](../../diagnostics/incompatible_engine_version.md).
Its target/source expressions, action values, CTEs and RETURNING use shared
name, type and parameter diagnostics; consult the linked owning pages. There is
no dedicated `merge_semantics` code to look up. Some unsupported MERGE paths
fail at an unimplemented analyzer boundary rather than producing a new
structured diagnostic.
