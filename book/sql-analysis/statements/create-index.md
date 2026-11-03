# CREATE INDEX

## Overview

PostgreSQL index analysis resolves the owning table and analyzes expression
keys and a partial-index predicate against that table's source scope. Plain key
and INCLUDE column names, key ordering, uniqueness and supported B-tree storage
settings are retained for later schema
analysis, including [ALTER INDEX](alter-index.md). An index name uses its owning
table's schema. IF NOT EXISTS follows the [shared relation-creation
policy](../statements.md).

```sql
CREATE UNIQUE INDEX items_label ON public.items (label) WITH (fillfactor = 90);
```

Expression keys and predicates use shared [expression](../expressions.md)
analysis. Plain key and INCLUDE names are retained without the same column
existence lookup; their presence does not establish that those columns were
validated. Retaining an index does not build it, check current data for
uniqueness, or establish that every runtime access-method property was
validated. Its retained keys can inform supported INSERT conflict inference; the
[INSERT catalog-service boundary](insert.md) still applies.

## Dialect and engine-specific behavior

SQLite CREATE INDEX semantic analysis remains unimplemented; receiving or
parsing such a statement is not equivalent support. PostgreSQL's supported
storage initialization is B-tree-specific. Sort USING expressions and other
missing index-analysis paths can fail before a retained index is produced.
Syntax options such as concurrency, tablespace and null-distinctness must not be
treated as executed or fully validated merely because they are retained in
syntax.

Index-element collation and operator-class references still lack complete
semantic binding. Concurrency, NULLS NOT DISTINCT and tablespace properties
also do not have complete analyzed application
([#547](https://github.com/kazuk/sql-fun/issues/547)). Expression-key scope,
partial predicates and supported B-tree storage settings are already analyzed;
the remaining modifier gap does not mean those implemented parts are absent.
SQLite/Turso DROP INDEX and index maintenance are separately unimplemented in
the [statement support inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).

## Diagnostics

- [relation_not_found](../../diagnostics/relation_not_found.md): the owning
  table cannot be resolved.
- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  requested index identity conflicts with an existing relation.
- [index_invalid_alteration](../../diagnostics/index_invalid_alteration.md):
  shared B-tree storage-option validation rejects a checked option or value
  during initialization.

Expression diagnostics from keys and predicates retain their own codes.
