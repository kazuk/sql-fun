# CREATE VIEW

## Overview

A view retains its analyzed SELECT and ordered result columns. An explicit
column-name list renames outputs by position; too many names or duplicate output
names are rejected. The query uses the shared [SELECT](select.md),
[clauses](../clauses.md) and [expressions](../expressions.md) rules. Catalog
application publishes an applicable definition for later statements. A
compatible PostgreSQL OR REPLACE preserves retained relation state and owner; an
incompatible result-column replacement leaves the prior definition intact.

```sql
CREATE VIEW public.item_names AS SELECT id, label FROM public.items;
```

### Writable views

Ordinary view writes are analyzed from the schema declarations you explicitly
load. PostgreSQL supports automatic writes through a single eligible relation,
matching INSTEAD OF triggers, and unconditional INSTEAD rules. INSERT and UPDATE
are independent: an INSERT trigger does not permit UPDATE. SQLite requires a
matching INSTEAD OF trigger; an UPDATE OF trigger requires at least one of its
listed columns in SET. Materialized views remain read-only.

Automatic views retain direct-column aliases through nested views. DISTINCT,
GROUP BY, HAVING, window functions, set-returning projections, joins, set
operations, WITH, LIMIT and OFFSET prevent automatic writes. Computed columns
cannot be assigned, even with DEFAULT, but remain available in RETURNING.
Assignable columns retain effective base defaults and generated/identity
restrictions; ALTER VIEW column defaults override base defaults. PostgreSQL
expands view UPDATE DEFAULT before checking base generated/ALWAYS identity
columns, so these assignments are rejected through automatic views even though
the corresponding table UPDATE DEFAULT and view INSERT DEFAULT are allowed.
Trigger/rule NEW-row fields have their own assignment semantics.

RETURNING is checked independently. An applicable INSTEAD rule requires an
unconditional replacement action with RETURNING. Conditional INSTEAD rules block
automatic writes; an INSTEAD OF trigger can still permit the operation.
Additional DO ALSO rules can coexist with automatic writes. For automatic
PostgreSQL views, ON CONFLICT inference maps aliases to real underlying
arbiters. Trigger-backed views allow targetless DO NOTHING without exposing base
indexes. Rule replacement and trigger paths with INSERT/UPDATE rules reject ON
CONFLICT. SQLite views reject UPSERT. Modifier and partial-index inference
retains the [INSERT catalog-service limitations](insert.md#overview).

Load trigger/rule declarations, replacements, drops, and view defaults after the
view declaration, in schema order. Catalog changes in a later schema layer do
not mutate a frozen parent. Invalid view declarations produce structured
semantic diagnostics and leave the existing definition intact. If a catalog
reports `view_missing_write_metadata`, rebuild it from complete schema
declarations before analyzing writes. Analysis does not execute trigger
functions, rule actions, CHECK OPTION checks, or runtime row constraints.
Trigger/rule SQL cannot currently be rendered back through the common AST
deparser. Broader rule and ALTER VIEW processing remains tracked separately.

### SQLAST API: evaluating view writes

API consumers construct `ViewWriteEvaluation::new(&mut context)` with the
existing analysis context, then call `insert`, `update`, or `evaluate` with an
already-bound View `FullName`. Evaluation follows the View's effective semantic
Trigger/Rule references and does not register capabilities or resolve a new
search path. `ViewInsertSupport::analyze` and `ViewUpdateSupport::analyze` use
this same service. The service reports semantic problems through that context
and returns `Ok(None)` when evaluation cannot finish because of a semantic
problem. A read-only view still yields `Ok(Some(support))`; a write to that view
reports a problem. Invalid statements leave an explicitly unresolved result so
later statements can still be analyzed. `AnalysisError` is reserved for
unrecoverable failures, including internal parser/analysis contract violations
and diagnostic delivery failures. Frozen contexts store analysis results;
writable-view analysis requires an active `ParseContext`.

The analyzer retains write capabilities in `InsertStatement::view_support` and
`UpdateStatement::view_support` for SQLAST API callers.

## Dialect and engine-specific behavior

PostgreSQL and SQLite both have CREATE VIEW analysis, with the different write
mechanisms described above. SQLite relation names use ASCII case-insensitive
lookup. Materialized views remain read-only; PostgreSQL materialized-query
definitions are handled through the table-as path described at [CREATE
TABLE](create-table.md#create-table-as). Analysis does not enforce runtime CHECK
OPTION checks or execute the defining query.

SQLite/Turso CREATE MATERIALIZED VIEW and DROP VIEW semantic entry points remain
unimplemented; see the
[statement support inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).
These paths do not acquire PostgreSQL semantics from similarly named commands.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): creation or
  replacement conflicts with an existing identity or incompatible output shape.
- [view_invalid_target](../../diagnostics/view_invalid_target.md): a checked
  view-write target is invalid.
- [view_missing_write_metadata](../../diagnostics/view_missing_write_metadata.md):
  effective semantic definitions needed to evaluate a write are unavailable.

Query diagnostics come from the shared SELECT/clause/expression paths. A write
through the view can also emit its INSERT or UPDATE diagnostic; declaration
success alone does not prove writability.
