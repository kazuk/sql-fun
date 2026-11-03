# query_invalid_cte_placement

## Code and severity

Code: `query_invalid_cte_placement`. Severity: Error.

## When it is reported

A data-modifying WITH body is nested where it must be top-level, or a WITH body
contains forbidden modifying WITH/SELECT INTO placement.

## What analysis knows and does not know

The invalid placement is known. Retaining the statement for analysis does not
mean the CTE can execute or publish its catalog effects.

## How to address it

Place the data-modifying WITH at the top level when that expresses the intended
SQL, and remove SELECT INTO from a forbidden WITH body.

## Example

PostgreSQL 18; selected schema contains `docs_items(id integer)`. SQLite CTE
modification syntax differs.

```sql
SELECT * FROM (
  WITH d AS (DELETE FROM docs_items RETURNING id) SELECT * FROM d
) AS s;
```

The DELETE CTE is nested inside a subquery. A top-level `WITH d AS (DELETE FROM
docs_items RETURNING id) SELECT * FROM d` has the required placement.

## Related diagnostics

[unsupported_statement](unsupported_statement.md) reports conversion cannot
represent a statement. [query_analysis](query_analysis.md) reports the analysis
pipeline fails for another reason.
