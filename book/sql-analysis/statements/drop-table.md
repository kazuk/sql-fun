# DROP TABLE

## Overview

DROP TABLE analysis retains the requested table names, IF EXISTS and dependency
choice. The current schema catalog appends the drop as metadata on each found
table; it does **not** remove a table definition or free its name for another
CREATE.

```sql
DROP TABLE public.items;
```

Supply the intended schema definitions rather than treating schema files as a
complete migration execution history. No database table or dependency is
removed, and CASCADE does not execute dependency removal.

## Dialect and engine-specific behavior

PostgreSQL and the feature-gated SQLite table paths retain DROP TABLE metadata.
IF EXISTS is retained, but an absent table is still diagnosed by the current
lookup/application path; do not infer database-style suppression from the syntax
flag. Other DROP object types have separate support boundaries; rule and trigger
drops are documented on their owning pages.

## Diagnostics

- [table_not_found](../../diagnostics/table_not_found.md): a requested table
  cannot be resolved on the DROP lookup/application path.
