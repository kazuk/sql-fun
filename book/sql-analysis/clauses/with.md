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
supported nonrecursive CTEs participate in SELECT and DML analysis. Recursive
analysis remains incomplete: recognition of `WITH RECURSIVE` does not establish
recursive name resolution, SEARCH/CYCLE semantics, or recursive type checking.
MERGE with recursive WITH remains unsupported.

With the `sqlite` feature, SQLite supports nonrecursive SELECT CTEs and WITH
before INSERT, UPDATE and DELETE. SQLite does not permit these modifying
statements as CTE bodies. library analysis support does not establish that the
CLI/server supports the same datasource initialization path; see the [analysis
overview](../../sql-analysis.md).

## Diagnostics

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

Missing RETURNING, unsupported body forms and recursive analysis can also stop
at an unimplemented or analysis-error boundary; a structured diagnostic is not
promised for every unsupported CTE. Rule-local OLD/NEW conflicts use the
[rules](../statements/rules.md) diagnostics. Body expressions retain their own
diagnostics.
