# SQL access-purpose reference

The [CRUD/query inventory](cli/check.md#crudquery-inventory) and
[`sql-fun analyze`](cli/analyze.md#relation-and-column-access) report why a SQL
statement accesses each SQL-visible relation and column. These purposes use the
exact public `AccessKind` names below. For supported SQL forms and diagnostic
limitations, see [SQL analysis coverage and limitations](sql-analysis.md).

A **relation-level** fact describes a row action on a table, view, materialized
view, or table-function output, without naming a column. A **column-level** fact
names a resolved column and describes its use. Relation actions do not imply
access to every column. `InsertTarget` and `UpdateTarget` identify column
write destinations; other column purposes identify value reads. A column-value
read also contributes a relation-level `ReadRows` fact for its supplying object.

Purposes describe static SQL references, including clauses and actions that may
never execute. They do not measure runtime access, count executions, prove
permissions, or expand database-side implementation details. Incomplete analysis
can return partial facts; an empty collection is not proof that SQL accesses
nothing. Check the command result and diagnostics as well.

## All access purposes

The scope column distinguishes relation row actions from column purposes. Clause
names describe the enclosing top-level operation: the
[nesting rules](#nested-expressions-subqueries-and-ctes) below also apply.

<!-- access-kind-reference:start -->

| AccessKind | Scope | Meaning | SQL clause or action |
| --- | --- | --- | --- |
| `InsertRows` | Relation | Insert rows into the destination. | `INSERT INTO`; a `MERGE` INSERT action. This is not schema-object creation. |
| `ReadRows` | Relation | Read rows from a SQL-visible source, or read a column value from it. | Sources in `FROM`/`USING`, table functions, and column-value reads; even `SELECT 1 FROM items` reads rows. |
| `UpdateRows` | Relation | Update destination rows. | `UPDATE`; a `MERGE` UPDATE action. |
| `DeleteRows` | Relation | Delete destination rows. | `DELETE FROM`; a `MERGE` DELETE action. This does not manufacture column writes. |
| `SelectOutput` | Column | Compute SELECT result values. | SELECT output expressions, including `*` and whole-row references. |
| `SelectSource` | Column | Evaluate a SELECT source expression. | Expressions in `FROM`, including function arguments and derived-query inputs. |
| `SelectFilter` | Column | Decide which SELECT rows pass. | SELECT `WHERE`. |
| `SelectJoin` | Column | Match SELECT source rows. | JOIN `ON`, `USING`, and the matching columns of `NATURAL JOIN`. |
| `SelectGrouping` | Column | Form SELECT groups. | `GROUP BY`, including references to output expressions. |
| `SelectGroupFilter` | Column | Decide which SELECT groups pass. | `HAVING`. |
| `SelectOrdering` | Column | Order SELECT results. | Query `ORDER BY`, including output aliases and ordinal positions. |
| `SelectDistinct` | Column | Choose distinct SELECT rows. | `DISTINCT` uses output dependencies; `DISTINCT ON` uses its listed expressions. |
| `SelectWindow` | Column | Define a named SELECT window. | Expressions in a named `WINDOW` definition, including partitioning, ordering, and frame bounds. |
| `SelectLimit` | Column | Determine how many SELECT result rows to keep. | `LIMIT` or the corresponding `FETCH` row-count expression, including nested column references. |
| `SelectOffset` | Column | Determine how many SELECT result rows to skip. | `OFFSET`, including nested column references. |
| `InsertTarget` | Column | Write an insertion destination column. | Explicit or implicit INSERT destination columns, including `DEFAULT VALUES`; destinations in a `MERGE` INSERT action. |
| `InsertInput` | Column | Supply inserted values. | INSERT `VALUES` or query inputs; value expressions in a `MERGE` INSERT action. |
| `InsertReturning` | Column | Compute INSERT result values. | INSERT `RETURNING`. |
| `InsertConflict` | Column | Select conflicts for an insertion. | Bound expressions in `ON CONFLICT ... DO NOTHING` selection, including conflict-target expressions and a partial-index predicate. |
| `UpdateTarget` | Column | Write an assignment destination. | Left side of `UPDATE SET`; assignments in a `MERGE` UPDATE action. |
| `UpdateInput` | Column | Compute an assignment value. | Right side of `UPDATE SET`; assignment inputs in a `MERGE` UPDATE action. |
| `UpdateSource` | Column | Evaluate an UPDATE source expression. | Expressions in UPDATE `FROM`. |
| `UpdateFilter` | Column | Decide which rows to update. | UPDATE `WHERE`. |
| `UpdateJoin` | Column | Match rows within UPDATE sources. | JOIN predicates in UPDATE `FROM`. |
| `UpdateReturning` | Column | Compute UPDATE result values. | UPDATE `RETURNING`. |
| `DeleteSource` | Column | Evaluate a DELETE source expression. | Expressions in DELETE `USING`. |
| `DeleteFilter` | Column | Decide which rows to delete. | DELETE `WHERE`. |
| `DeleteJoin` | Column | Match rows within DELETE sources. | JOIN predicates in DELETE `USING`. |
| `DeleteReturning` | Column | Compute DELETE result values. | DELETE `RETURNING`. |
| `MergeSource` | Column | Evaluate a MERGE source expression. | Expressions in MERGE `USING`. |
| `MergeJoin` | Column | Match MERGE source and target rows. | MERGE `ON` and JOIN predicates within its source. |
| `MergeCondition` | Column | Decide which MERGE action applies. | Additional `WHEN ... AND` conditions. |
| `MergeReturning` | Column | Compute MERGE result values. | MERGE `RETURNING`, where supported by the selected PostgreSQL version. |

<!-- access-kind-reference:end -->

These purposes do not imply that every expression names a column: a constant
`LIMIT 10`, for example, adds no column fact. A scalar subquery in LIMIT or OFFSET
can reference columns and give those columns the corresponding purpose.
Aggregate arguments, aggregate `FILTER`/ordering, and inline window expressions
inherit the purpose of the expression containing them. For example, a window
expression in SELECT output has `SelectOutput`; a separate named `WINDOW`
definition has `SelectWindow`.

MERGE has no separate insertion/update column purposes: its actions reuse
`InsertTarget`, `InsertInput`, `UpdateTarget`, and `UpdateInput`. Its relation
facts reflect the INSERT, UPDATE, and DELETE actions present in the SQL, rather
than which actions would execute for particular data. `DO NOTHING` adds no row
write action.

## Reading common distinctions

The following examples assume tables `items(id integer, value integer)` and
`other_items(id integer, value integer)`. Relation names below omit schema
qualification for readability; reports preserve resolved qualification.

### Row reads versus output values

```sql
SELECT 1 FROM items;
SELECT id FROM items;
```

The first statement has `ReadRows` on `items` and no column facts. The second has
both `ReadRows` on `items` and `SelectOutput` on `items.id`. Reading rows does not
mean that every column is an output, or that any column must be referenced.

### Output versus filtering

```sql
SELECT id FROM items WHERE id > 0;
```

`items.id` has both `SelectOutput` and `SelectFilter`. The relation also has
`ReadRows`. The two column purposes remain separate even though they refer to
the same column in one statement. Identical facts within one purpose are
deduplicated; distinct purposes are retained.

### Assignment destinations versus inputs

```sql
UPDATE items SET value = value + 1 WHERE id = 1 RETURNING value;
```

| Object | Purposes |
| --- | --- |
| Relation `items` | `UpdateRows`, `ReadRows` |
| Column `items.value` | `UpdateTarget`, `UpdateInput`, `UpdateReturning` |
| Column `items.id` | `UpdateFilter` |

`UpdateTarget` identifies the written column. `UpdateInput` identifies the old
value read to compute its replacement. `UpdateReturning` identifies the value
requested as output. A constant assignment such as `SET value = 1` still has
`UpdateTarget`, but the constant supplies no column-level `UpdateInput` fact.
Likewise, `DELETE FROM items` has `DeleteRows` without inventing column accesses.

## Nested expressions, subqueries, and CTEs

The enclosing top-level clause determines a nested reference's purpose. Inner
clause spelling does not replace that purpose:

```sql
SELECT (
  SELECT other_items.value
  FROM other_items
  WHERE other_items.id = items.id
)
FROM items;
```

All three referenced columns have `SelectOutput`: `other_items.value`,
`other_items.id`, and the correlated `items.id`. Both relations have `ReadRows`.
The inner WHERE contributes to computing the outer output; it does not assign
`SelectFilter` to those columns.

```sql
SELECT id FROM items
WHERE id = (
  SELECT value FROM other_items WHERE other_items.id = 1
);
```

Here `other_items.value` and `other_items.id` have `SelectFilter`. `items.id`
has both `SelectOutput` and `SelectFilter`.

Aliases, derived-table outputs, and CTE outputs resolve to their contributing
SQL-visible inputs, rather than becoming new persistent relation identities:

```sql
WITH c AS (SELECT id, value FROM items)
SELECT c.id FROM c WHERE c.value > 0;
```

The output reference contributes `SelectOutput` on `items.id`, and the filter
contributes `SelectFilter` on `items.value`. Evaluating the CTE definition in the
outer source also contributes `SelectSource` on its referenced columns, and
`items` has `ReadRows`; there is no schema relation named `c` in the report.
Recursive CTE output references also trace contributing inputs through recursive
references without expanding them indefinitely. Set-operation outputs retain
contributors from their operands. Data-modifying CTEs retain row actions, while
nested column uses inherit the enclosing top-level purpose and CTE output uses
contribute the consumer's purpose.

## SQL-visible object boundaries

A view reference identifies that view and its columns; a materialized view is
also reported as its own SQL-visible object. Stored view definitions are not
expanded into base-table accesses. Table-function outputs identify the resolved
function and its declared output columns. Function bodies are not inspected;
argument expressions written in the submitted SQL are analyzed.

For example, `SELECT id FROM visible_view` reports the view and its `id` output,
not the tables inside its stored definition. These boundaries make this a
query/schema usage report, rather than a complete inventory of database-internal
side effects.
