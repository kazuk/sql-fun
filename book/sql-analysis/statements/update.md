# UPDATE

## Overview

UPDATE analysis binds the destination, assignments, optional FROM sources, WHERE
predicate and RETURNING expressions. SET right-hand expressions use the original
visible row values; assignments are not executed in written order. Assignment
destinations must exist and duplicate or overlapping destinations are checked.
Array subscripts, slices and composite-field destinations are analyzed through
the shared structural expression rules.

```sql
UPDATE public.items AS i SET label = $1 WHERE i.id = $2 RETURNING i.id;
```

PostgreSQL assignment conversions constrain inputs. Whole-column DEFAULT remains
distinct from an expression; a generated or GENERATED ALWAYS identity column
accepts only its allowed DEFAULT assignment. DEFAULT is not accepted for a
partial column destination. Multi-column assignments retain their row/subquery
source and check arity; they do not make a general row value legal in every
scalar context. Analysis does not evaluate defaults or validate runtime row
constraints.

The target alias controls its visibility. FROM subqueries cannot implicitly
correlate to the UPDATE destination; supported right-hand expressions use the
completed source and target scope. PostgreSQL ONLY/inheritance and WHERE CURRENT
OF are represented, but analysis does not verify a cursor's runtime position or
lifetime. Explicit SQLite INDEXED BY must identify an index of the destination.

Without RETURNING there are no result columns. Shared
[WITH](../clauses/with.md), [FROM](../clauses/from.md),
[WHERE](../clauses/where.md), [RETURNING](../clauses/returning.md) and
[parameter](../expressions/parameters.md) rules apply. For write-capable views,
assignment/default eligibility and effective triggers/rules are described at
[CREATE VIEW](create-view.md#writable-views).

## Dialect and engine-specific behavior

PostgreSQL supports UPDATE FROM, assignment casts, DEFAULT and positioned-update
analysis along the implemented paths. SQLite retains its OR conflict policy and
index hints and uses dynamic typing rather than PostgreSQL assignment casts.
SQLite generated columns cannot be assigned, including via PostgreSQL's DEFAULT
exception. SQLite RETURNING uses the original destination name and excludes FROM
sources and qualified stars; PostgreSQL RETURNING uses the visible target/source
scope.

SQLite UPDATE ORDER BY/LIMIT is rejected by the inherited parser even if the
database was built with SQLITE_ENABLE_UPDATE_DELETE_LIMIT. PostgreSQL 18
renamed/destination RETURNING row-image binding remains unimplemented
([#621](https://github.com/kazuk/sql-fun/issues/621)). Supported UPDATE analysis
does not establish support for ON CONFLICT DO UPDATE, which has its own
unimplemented INSERT path ([#1024](https://github.com/kazuk/sql-fun/issues/1024)).

## Diagnostics

- [relation_not_found](../../diagnostics/relation_not_found.md): the destination
  relation cannot be resolved.
- [column_not_found_in_table](../../diagnostics/column_not_found_in_table.md): a
  SET destination column is absent.
- [update_semantics](../../diagnostics/update_semantics.md): a checked target
  capability, repeated/overlapping assignment, assignment arity/conversion,
  generated/identity/DEFAULT rule, WHERE predicate, cursor, index hint or
  RETURNING requirement fails.
- [view_missing_write_metadata](../../diagnostics/view_missing_write_metadata.md):
  effective semantic view-write metadata is unavailable.

Shared expression and name-resolution findings are described at their owning
[expression pages](../expressions.md). A parser or analyzer failure is distinct
from a structured semantic diagnostic.
