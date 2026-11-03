# DROP TABLE

## Overview

DROP TABLE analysis looks up the requested tables but does **not** remove their
definitions or free their names for another CREATE during subsequent schema
analysis.

```sql
DROP TABLE public.items;
```

Supply the intended schema definitions rather than treating schema files as a
complete migration execution history. No database table or dependency is
removed, and CASCADE does not execute dependency removal.

## Dialect and engine-specific behavior

For PostgreSQL and the feature-gated SQLite analysis, an absent table still
produces a diagnostic even with IF EXISTS; do not infer database-style
suppression from the syntax flag. Other DROP object types have separate support
boundaries; rule and trigger drops are documented on their owning pages.

## Diagnostics

- [table_not_found](../../diagnostics/table_not_found.md): a requested table
  cannot be resolved on the DROP lookup/application path.
