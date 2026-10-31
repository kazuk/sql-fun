# PostgreSQL 18 parser migration

This maintainer record maps the grammar changes in issue #885 to syntax
retention, analysis notifications and maintained regression coverage. The parser
is `pg_query` 18.1.0 (embedded PostgreSQL 18.6); the selected analysis target still
comes from `ParseContext` metadata. Installing that parser does not select a
PostgreSQL version for a user.

## Grammar inventory

Compared PostgreSQL's `REL_17_STABLE` and `REL_18_STABLE` `src/backend/parser/gram.y`
and cross-checked the [PostgreSQL 18 release notes](https://www.postgresql.org/docs/18/release-18.html).
The paired `postgres18-syntax-17.json` / `postgres18-syntax-18.json` CLI fixtures
run through the shared source-tree and installed-distribution manifest.

| New form | Representation and notification consumer |
| --- | --- |
| `RETURNING WITH (OLD AS ..., NEW AS ...)` | `ReturningClause.options` becomes ordered common-AST `ReturningOption` values; DML and CTE analysis notify before binding. Ordinary `ReturningClause.exprs` remains valid on 17. |
| RETURNING `old` / `new` row images | Existing column and source namespace lookup distinguishes real columns/aliases from special row images. Scalar and star interpretation notify before image binding. Existing rule OLD/NEW scopes remain distinct. |
| `GENERATED ... VIRTUAL`, or omitted storage kind | `Constraint.generated_kind` retains the parser's virtual mode. Table/ALTER column analysis gates it; explicit `STORED` remains valid on 17. Identity generation continues using `generated_when`. |
| Key `WITHOUT OVERLAPS`; foreign-key `PERIOD` | Constraint extras retain temporal flags, and key order identifies the period column. Constraint analysis notifies before ordinary key semantics could erase the distinction. |
| Constraint `ENFORCED` / `NOT ENFORCED` | Inline attributes remain separate constraint nodes; table attributes remain on their owner. Token positions distinguish explicit table `ENFORCED` from the parser's default `is_enforced = true`. |
| Table `NOT NULL column`, inline `NOT NULL NO INHERIT`, new nullability validation attributes | Nullability constraints retain keys, inheritance and validation flags; statement/ALTER analysis gates the new forms. Plain inline NOT NULL remains valid on 17. |
| `ALTER CONSTRAINT` enforcement and `INHERIT` / `NO INHERIT` | `ATAlterConstraint` retains independent optional deferrability, enforcement and inheritance changes. Only the new attributes trigger version notifications. |
| `ALTER DEFAULT PRIVILEGES ... ON LARGE OBJECTS` | Common-AST conversion explicitly panics with `todo!()`; conversion and interpretation are tracked in #891. |
| `CREATE FOREIGN TABLE (... LIKE ...)` | Common-AST conversion explicitly panics with `todo!()` (#891). PostgreSQL 17 explicitly prohibited this combination. |

Keyword additions (`ENFORCED`, `OBJECTS`, `PERIOD`, `VIRTUAL`) and parser source
location changes are not independent feature choices. Their containing constructs
supply the version requirement. The IN/subquery grammar refactor and new source
range fields do not introduce a new SQL form. `ReturningExpr`, `CompareType`,
`JoinRightSemi`, `VariableSetStmt.jumble_args` and `CreateStmt.nnconstraints`
also describe analyzed/internal
trees; raw syntax must not be inferred from their existence. `SummaryResult`
belongs to a separate libpg_query API, as described in [GENERATION.md](GENERATION.md).

Small rolled-back PostgreSQL 17.6/18.1 oracle probes confirmed the new RETURNING
alias clause, virtual/implicit generated modes, enforcement, new NOT NULL forms,
large-object default privileges and foreign-table LIKE. An ordinary target alias
`old` remained valid on both engines. No regression test depends on these servers.

## Remaining semantic work

On implemented conversion paths, syntax on a PostgreSQL 17 target reports
`incompatible_engine_version` through `ParseContext`. Compatible syntax whose
analysis is missing reaches an explicit `todo!()` panic. Missing utility
conversion also panics directly; no conversion-error feature metadata,
alternative unsupported diagnostic or success-shaped placeholder is used.
The version diagnostic uses the existing analysis protocol.

The migration retains parse/edit/deparse syntax but does not implement:

- Renamed RETURNING row-image binding, INSERT/UPDATE destination row images, or
  whole-row image values (related work: #621). Existing DELETE/MERGE scalar image
  bindings remain in their responsible analyzers.
- Temporal key/foreign-key enforcement and period semantics.
- Non-enforced constraint catalog semantics; ALTER CONSTRAINT enforcement or
  inheritance transitions.
- New table/nullability inheritance and validation semantics.
- Common-AST interpreters for the two utility statement families above, which
  were already unsupported before this upgrade.

Constraint gaps are tracked by #887. Utility conversion and interpretation are
tracked by #891, and domain-constraint analysis by #548. Paired CLI cases cover
implemented paths and target-version diagnostics; focused Rust tests verify explicit
unimplemented panics without terminating the shared CLI regression server.

## Independent existing limitation observed during acceptance

`CREATE TABLE g (id int, computed int GENERATED ALWAYS AS (id+1) STORED)` and
inline `CHECK (id>0)` currently report `column_not_found_in_source` when resolving
the new declaration's own column. The unchanged column-constraint interpreter
uses an empty scalar scope before the new table is registered. This also affects
the pre-existing STORED form on a PostgreSQL 17 target; repairing declaration
scope is independent of the parser migration. Version-gating CLI cases use
constant expressions to exercise the new modes without asserting that incorrect
diagnostic as a contract. Native-tree round-trip cases still include column
references and verify their syntax is retained. No declaration-scope repair is
included in #885.
