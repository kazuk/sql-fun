# WITH

## Overview

A nonrecursive `WITH` defines named query results for the statement and later
CTEs. Bodies are analyzed in declaration order; explicit CTE column aliases
rename the exposed outputs. Nested queries share the statement's parameter
numbering and explicit input hints. Repeated parameters contribute to the same
[parameter constraints](../expressions/parameters.md).

```sql
WITH first_value AS (SELECT 1 AS n), next_value AS (SELECT n FROM first_value)
SELECT n FROM next_value;
```

PostgreSQL INSERT, UPDATE, DELETE and MERGE bodies expose only their
[RETURNING](returning.md) columns. A modifying CTE without RETURNING may be
unreferenced, but cannot be read as a relation. A data-modifying CTE must be at
the top level; a nested modifying body is invalid. Materialization annotations
do not cause sql-fun to evaluate the body or produce an execution plan.
[Reusable CTE](../../reusable_cte.md) describes the separate application
workflow.

## Dialect and engine-specific behavior

PostgreSQL MERGE bodies require configured PostgreSQL 17 or later. Ordinary
supported CTEs participate in SELECT and DML analysis. `WITH RECURSIVE` allows
forward references between CTEs. A recursive CTE uses `UNION` or `UNION ALL`
between a nonrecursive term and a recursive term. The anchor supplies the
columns visible to the recursive term. Explicit aliases rename leading columns.
PostgreSQL analysis checks that the overall result types agree with the anchor.
Both terms share the enclosing statement's parameter constraints. A recursive
column reference has unknown nullability during analysis because later
iterations can produce values absent from the anchor.

```sql
WITH RECURSIVE counter(n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM counter WHERE n < 3
)
SELECT n FROM counter;
```

PostgreSQL validation of recursive output collations and type modifiers such as
`varchar(10)` remains incomplete. See [#1137](https://github.com/kazuk/sql-fun/issues/1137)
and [#1138](https://github.com/kazuk/sql-fun/issues/1138). `SEARCH` and `CYCLE`
generated columns and `MERGE` with recursive `WITH` remain unsupported
([#1140](https://github.com/kazuk/sql-fun/issues/1140)).

With the `sqlite` feature, SQLite supports SELECT CTEs and WITH before INSERT,
UPDATE and DELETE. SQLite does not permit these modifying statements as CTE
bodies. A single recursive SELECT with an explicit `WITH RECURSIVE` uses the
shared CTE lookup and column binding. Expressions in each term still depend
on SQLite expression support, including [operator analysis tracked in #504](https://github.com/kazuk/sql-fun/issues/504).
Static output metadata follows the
existing SQLite compound-query rules. SQLite can return different storage
classes in one result column. SQLite requires a direct FROM reference in
the recursive SELECT, while PostgreSQL can resolve the reference through a
derived table. SQLite permits the recursive table on the nullable side of
an outer join, where PostgreSQL rejects it. Multiple recursive SELECT arms,
recursion without the keyword, and SQLite-specific alias-count and recursive
ordering/limit rules are tracked in [#1139](https://github.com/kazuk/sql-fun/issues/1139).
See the [analysis overview](../../sql-analysis.md) for CLI datasource support.

## Diagnostics

- [query_invalid_recursive_cte](../../diagnostics/query_invalid_recursive_cte.md)
  reports invalid recursive references, cycles, or incompatible recursive
  output definitions.
- [query_invalid_cte_placement](../../diagnostics/query_invalid_cte_placement.md):
  a modifying CTE is nested where top-level placement is required, or a CTE body
  has forbidden modifying-WITH/SELECT-INTO placement.
- [column_not_found_in_cte](../../diagnostics/column_not_found_in_cte.md):
  the named CTE has no requested output column.
- [sub_query_not_have_column_for_derived_column_name](../../diagnostics/sub_query_not_have_column_for_derived_column_name.md):
  a derived alias position has no underlying result column.
- [insert_semantics](../../diagnostics/insert_semantics.md),
  [update_semantics](../../diagnostics/update_semantics.md) and
  [delete_semantics](../../diagnostics/delete_semantics.md): statement-specific
  CTE restrictions, including more declared aliases than RETURNING outputs;
  DELETE also diagnoses a SQLite modifying CTE at its semantic boundary.
- [incompatible_engine_version](../../diagnostics/incompatible_engine_version.md):
  a version-gated feature is unavailable for the selected engine version.

Missing RETURNING and unsupported body forms can also stop
at an unimplemented or analysis-error boundary; a structured diagnostic is not
promised for every unsupported CTE. Rule-local OLD/NEW conflicts use the
[rules](../statements/rules.md) diagnostics. Body expressions retain their own
diagnostics.
