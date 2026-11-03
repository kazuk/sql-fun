# SELECT

## Overview

SELECT analysis resolves visible source names and supported expressions, then
describes ordered result-column names, types and nullability. Explicit AS names
make the intended output names clear. [Name resolution](../expressions/names.md)
owns qualification, alias hiding, composite member access and projection
expansion; [FROM](../clauses/from.md) owns sources and joins.

```sql
SELECT i.id AS item_id, i.label FROM public.items AS i WHERE i.id = $1;
```

WITH, WHERE, GROUP BY, HAVING, WINDOW/OVER, ORDER BY and LIMIT/OFFSET have
[shared clause pages](../clauses.md). Binding names in a clause does not
establish complete runtime aggregate/window legality. Supported DISTINCT and
DISTINCT ON expressions participate in analysis and parameter constraints.

### Set operations

UNION, INTERSECT and EXCEPT retain their operands and ALL modifier. Operands
must have equal output widths. PostgreSQL selects a common result type for each
column using the shared [type rules](../expressions/types-and-casts.md);
deferred parameter-dependent results can retain several alternatives. Output
names follow the left operand. Result nullability combines the known operand
facts according to the operation rather than evaluating any rows. Invalid set
queries do not publish a view or materialized view through the schema
application path.

### Wildcard projection expansion

PostgreSQL datasources can opt in to expanding table-dependent wildcard
projections in executable SQL. Enable `rewrite_options.expand_asterisk = true`
in the [datasource configuration](../../installation.md). The option is disabled
unless explicitly true, and applies independently to each datasource.

For example, when the analyzed `users` table has columns `id` and `name`, this
query:

```sql
SELECT u.* FROM users AS u
```

produces executable SQL equivalent to:

```sql
SELECT u.id AS id, u.name AS name FROM users AS u
```

Expansion uses the supplied schema's column order and preserves output names,
including duplicate names. It covers table-dependent projections in nested
queries, CTEs, joins, and DML `RETURNING` clauses. Joined wildcard projections
preserve `USING` and `NATURAL JOIN` column order and merged values.

Only projections depending on a table's column set are expanded. `count(*)`,
composite-value expansion such as `(record_value).*`, and stars over fixed view,
derived-query, or table-function outputs remain unchanged. An inner
table-dependent star can be expanded while an outer star over its resulting
fixed projection remains unchanged.

The explicit projection keeps result columns stable across database environments
with added columns or dropped unreferenced columns. Every explicitly referenced
column must still exist: dropping a required column causes execution to fail.
Expansion does not make differently migrated schemas interchangeable in general.
SQLite expansion is not available.

## Dialect and engine-specific behavior

PostgreSQL and SQLite SELECT have separate dialect behavior. SQLite analysis
supports literal projections, derived-query and nonrecursive CTE columns, join
nullability, compound queries and compatible CASE results. SQLite compound
results retain the left output type rather than PostgreSQL common-type
coercions; width is checked, but full compound type validation remains
incomplete.

SQLite table/view column lookup uses ASCII case-insensitive matching, but
derived-query and CTE output column matching remains case-sensitive in some
paths. Use matching spelling for those outputs; see the exact [name-resolution
boundaries](../expressions/names.md). Arithmetic/comparison and
function-affinity behavior must be checked against the individual
[expression](../expressions.md) support boundaries rather than inferred from
PostgreSQL typing. Recursive CTE binding and full grouping/window legality
remain incomplete. SQLite syntax depth and UPDATE/DELETE grammar caveats are
documented at the [overview](../../sql-analysis.md). PostgreSQL-only
executable-star rewriting is separate from ordinary wildcard result analysis.

### VALUES result metadata

Multi-row VALUES does not yet reconcile output types and widths across every
row ([#565](https://github.com/kazuk/sql-fun/issues/565)). Its exposed output
metadata comes from the first row. For example, the output type of
`VALUES (1), (1.5)` is not established by analyzing a common type across both
rows. This also affects VALUES used as a derived source, set-operation operand
or INSERT source; traversing later rows does not make the result shape complete.

### Incomplete result-column descriptions

Result-column name and type reporting is incomplete for some expressions,
including SQL value expressions such as CURRENT_DATE
([#521](https://github.com/kazuk/sql-fun/issues/521)). Requesting a
result-column description through the analysis library can terminate analysis
even after the expression itself was accepted. This does not mean every CLI
projection containing such an expression fails.

## Diagnostics

- [query_set_column_count](../../diagnostics/query_set_column_count.md):
  set-operation operands have different result widths.
- [query_set_column_type](../../diagnostics/query_set_column_type.md):
  PostgreSQL set-result common typing fails or required type facts are
  unavailable.
- [query_invalid_star_projection_placement](../../diagnostics/query_invalid_star_projection_placement.md):
  star projection occurs in an expression position that does not allow it.
- [query_scalar_value_required](../../diagnostics/query_scalar_value_required.md):
  a row value is used where the dialect requires a scalar result.
- [query_star_projection_requires_composite_type](../../diagnostics/query_star_projection_requires_composite_type.md):
  a composite star expansion is requested from a known non-composite operand.

Clause and expression findings are linked from their owning pages. Inferred
result metadata can remain available even when a parameter input binding is
unresolved.
