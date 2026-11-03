# INSERT

## Overview

PostgreSQL and SQLite INSERT analysis retains the destination column bindings,
VALUES or query source, conflict policy, and RETURNING result names, types, and
nullability. Without RETURNING, the analyzed statement has no result columns.
DEFAULT VALUES and PostgreSQL per-value DEFAULT remain distinct from explicit
expressions; analysis does not evaluate database defaults or execute the INSERT.

Explicit destination columns follow the supplied order. PostgreSQL also permits
values for only the first columns when the column list is omitted; SQLite
requires values for every non-generated column in that form. Generated and
PostgreSQL identity columns are checked against DEFAULT and OVERRIDING rules.
PostgreSQL assignment conversions constrain INSERT inputs, including inputs from
SELECT sources. SQLite inputs retain SQLite's dynamic typing rather than using
PostgreSQL assignment casts; successful analysis does not guarantee that a value
passes runtime constraints, including STRICT table checks.

RETURNING resolves destination columns and expands `*`. PostgreSQL permits
`alias.*` and hides the original table name after an INSERT alias is supplied.
SQLite RETURNING uses the original table name and rejects qualified stars.
PostgreSQL INSERT CTEs expose their RETURNING columns to subsequent queries; a
CTE without RETURNING cannot be read as a relation. SQLite permits WITH before
INSERT but does not permit INSERT inside a CTE body.

DO NOTHING retains and checks conflict targets against catalog constraints and
unique indexes, including matching expression keys and identical partial-index
predicates. Named constraint targets require constraint names retained in the
schema; automatically generated database names are not inferred.

### SQLAST API: conflict catalog services

Explicit conflict collation/operator-class references retain their completed
catalog lookups, including missing and ambiguous outcomes. Custom analysis
contexts provide these lookups and modifier-aware arbiter inference through
`ConflictInfoRead`; SQL spellings are not treated as resolved identities.
Production PostgreSQL catalogs load collation and operator-class definitions
from `collations.json` and `operator_classes.json` in the explicitly selected
catalog generation. Both files are required; incomplete or malformed generations
fail to load. Name lookup honors qualification, the current search path,
database encoding, and the requested access method. `ConflictInfoRead` provides
purpose-specific `database_collation`, `collation_definition`, and
`operator_class_definition` reads for database-default provenance and qualified
definitions with operator-family/type relations. The provider selects
encoding/access-method eligibility; backing master collections remain private.
Frozen catalogs retain the same read services.

SQLite lookup uses its connection-local, case-insensitive collation namespace.
BINARY, NOCASE, and RTRIM are available. API consumers must explicitly declare
custom names with `BaseContext::declare_sqlite_collation` before freezing the
base; this declaration records availability for analysis and does not install a
runtime collation or inspect a connection.

Modifier-aware arbiter eligibility and general partial-index predicate
implication still require additional catalog services and can terminate analysis
at their catalog boundary. Their implementation and production INSERT end-to-end
verification remain tracked in
[#581](https://github.com/kazuk/sql-fun/issues/581). Master lookup support alone
does not establish those INSERT capabilities. DO UPDATE remains explicitly
unimplemented ([#474](https://github.com/kazuk/sql-fun/issues/474)); it does not
produce a successful INSERT analysis, including later actions in SQLite UPSERT
chains.

View-target eligibility and defaults are described once in [CREATE
VIEW](create-view.md#writable-views). Shared [WITH](../clauses/with.md),
[RETURNING](../clauses/returning.md), [type
conversions](../expressions/types-and-casts.md) and [parameter
inference](../expressions/parameters.md) apply to the supported sources and
outputs.

## Dialect and engine-specific behavior

Both PostgreSQL and SQLite have dedicated INSERT analysis. Their destination
widths, assignment rules and RETURNING scopes differ as described above. The
SQLite parser and expression limitations in the [reference
overview](../../sql-analysis.md) still apply. PostgreSQL 18 destination row
images remain a separate unimplemented boundary
([#621](https://github.com/kazuk/sql-fun/issues/621)); grammar recognition is
not support for binding them. Native SQLite analyzer support is not a claim that
the server initialization API supports SQLite.

## Diagnostics

- [relation_not_found](../../diagnostics/relation_not_found.md): the requested
  destination cannot be resolved.
- [column_not_found_in_table](../../diagnostics/column_not_found_in_table.md):
  an explicit destination column does not exist.
- [insert_semantics](../../diagnostics/insert_semantics.md): a checked
  destination/source width, duplicate target, generated/identity assignment,
  assignment conversion, view-write, RETURNING or conflict-arbiter rule fails.
- [view_missing_write_metadata](../../diagnostics/view_missing_write_metadata.md):
  the selected view lacks the effective semantic metadata required to evaluate
  writability.

Shared expression and name-resolution findings are described at their owning
[expression pages](../expressions.md). A parser or analyzer failure is distinct
from a structured semantic diagnostic.
