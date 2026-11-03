# ALTER INDEX

## Overview

Schema analysis supports `ALTER INDEX [IF EXISTS] name SET (...)` and `RESET
(...)` for built-in B-tree storage parameters: `fillfactor`,
`deduplicate_items`, and `vacuum_cleanup_index_scale_factor`. Settings supplied
by `CREATE INDEX ... WITH (...)` are retained for subsequent alterations.
`RESET` removes explicit overrides, including a successful no-op for an unknown
parameter name, following PostgreSQL behavior.

`ALTER INDEX [IF EXISTS] name ALTER [COLUMN] n SET STATISTICS value` supports
expression key columns, addressed by their one-based position in the index.
Ordinary key columns and included columns cannot receive index statistics. Use
`-1` to reset to the database's statistics default; values above 10000 are
lowered to 10000 with a warning. The analyzer does not choose the database's
configured default value.

Supported alterations resolve the target against the schema search path and
update the index definition used by subsequent schema statements. An invalid
statement leaves the catalog unchanged. `IF EXISTS` skips an absent target; it
does not permit a target of another relation kind.

Other access-method storage parameters, `SET TABLESPACE`, `RENAME`, extension
dependencies, `ALL IN TABLESPACE`, and `ATTACH PARTITION` remain unimplemented.
Recognizing their syntax does not imply semantic or catalog support.

## Dialect and engine-specific behavior

This page describes PostgreSQL schema analysis. SQLite CREATE/ALTER INDEX
semantic application is not established by recognizing an index statement. See
[CREATE INDEX](create-index.md) for retained creation metadata.

## Diagnostics

- [index_not_found](../../diagnostics/index_not_found.md): the requested index
  cannot be resolved.
- [index_invalid_alteration](../../diagnostics/index_invalid_alteration.md): a
  checked target kind, storage option, option value or statistics position is
  invalid.
- [index_statistics_clamped](../../diagnostics/index_statistics_clamped.md): a
  statistics target above 10000 is reduced with a warning.
