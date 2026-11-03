# LIMIT and OFFSET

## Overview

LIMIT and OFFSET expressions are analyzed separately from the query's row-source
scope. Their operands contribute to the statement's parameter type requirements.
PostgreSQL constrains row-count parameter inputs to bigint- compatible types.
Compound queries own their trailing LIMIT/OFFSET; parenthesized operand-local
clauses remain with that operand.

```sql
SELECT 1 LIMIT $1 OFFSET $2;
```

The analyzer does not execute the query, count its rows, or guarantee how many
rows it will return. Query limit modifiers, including WITH TIES, do not yet have
complete legality checking for every modifier or bound value.

## Dialect and engine-specific behavior

Parameters in supported SQLite SELECT limit expressions are analyzed
without PostgreSQL bigint coercion requirements. SQLite UPDATE/DELETE ORDER BY
or LIMIT forms are rejected by the inherited parser even on a SQLite build with
SQLITE_ENABLE_UPDATE_DELETE_LIMIT. There is no sql-fun parser setting or
preprocessing workaround that enables them. See the
[parser limitations](../../sql-analysis.md#sqlite-parser-limitations).

## Diagnostics

[parameter_constraint](../../diagnostics/parameter_constraint.md) reports
incompatible requirements shared with other uses of the same parameter.
[query_invalid_grouping_placement](../../diagnostics/query_invalid_grouping_placement.md)
reports GROUPING at a forbidden LIMIT/OFFSET query level. Unsupported source
references and bound expressions retain their own
[name](../expressions/names.md#diagnostics) and expression diagnostics.
