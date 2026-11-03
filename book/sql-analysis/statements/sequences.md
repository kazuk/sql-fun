# CREATE SEQUENCE / ALTER SEQUENCE

## Overview

CREATE SEQUENCE retains a PostgreSQL sequence identity, temporary/existence
flags and available OWNED BY table association. Catalog creation participates in
the shared relation namespace. ALTER SEQUENCE resolves its sequence target and
updates the retained OWNED BY association, including clearing it for OWNED BY
NONE.

```sql
CREATE SEQUENCE public.item_ids;
ALTER SEQUENCE public.item_ids OWNED BY NONE;
```

The association is used by later schema changes, such as table renames and
dependent ownership updates. Analysis never allocates or advances a sequence
value.

## Dialect and engine-specific behavior

The current metadata application is narrower than PostgreSQL's full sequence
command behavior: increment/start/minimum/maximum/cache/cycle options and
runtime sequence state must not be inferred as validated/applied from retained
syntax. ALTER SEQUENCE IF EXISTS syntax does not currently establish a
no-diagnostic absent-target behavior; the implementation still reports a missing
sequence. SQLite does not have this PostgreSQL sequence-analysis path.

SQLite/Turso CREATE SEQUENCE and DROP SEQUENCE semantic entry points remain
unimplemented; see the
[statement support inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).
These paths do not acquire PostgreSQL semantics from similarly named commands.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): CREATE
  SEQUENCE conflicts with an existing relation identity.
- [table_not_found](../../diagnostics/table_not_found.md): an OWNED BY table
  cannot be resolved.
- [sequence_not_found](../../diagnostics/sequence_not_found.md): ALTER SEQUENCE
  cannot resolve its sequence target.
