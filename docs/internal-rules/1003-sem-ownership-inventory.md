# #1003 semantic ownership baseline and final source map

This reviewer-facing ledger records the old AST source set at base revision
`bf81afb3e31fc03b4bc10e792d66b79b4601f535` and its disposition in the current
implementation worktree (2026-11-04). It uses the frozen source tree for
baseline facts and the actual current files for destinations. The 106-row table
contains the source paths, baseline test counts, and final destinations needed
for later reconciliation.

Issue #1003 requires destination-owned `dialect syntax → common AST → semantic
tree` processing and preservation of existing PostgreSQL 17/18 and SQLite
behavior. The source-to-destination mapping below reflects the completed
implementation layout, rather than the earlier plan's proposed module names.

## Baseline inventory

Scope: `sql-fun-sqlast/src/ast.rs` and all tracked Rust files under
`sql-fun-sqlast/src/ast/` at the base revision. Counts are source annotations
and assertion-macro calls, not compiled case totals.

| Old AST area | Files | Lines | Test functions | `#[case]` attrs | Assertion macros in attributed tests |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ast.rs` and top-level `ast/*.rs` | 21 | 9,686 | 77 | 198 | 232 |
| `ast/resolve.rs` and `ast/resolve/**` | 42 | 15,511 | 308 | 1,104 | 1,115 |
| `ast/ddl.rs` and `ast/ddl/**` | 7 | 4,059 | 14 | 47 | 59 |
| `ast/deparse/**` | 11 | 8,245 | 60 | 400 | 92 |
| `ast/document.rs` and `ast/document/**` | 8 | 2,506 | 26 | 62 | 73 |
| `ast/ownership.rs` and `ast/ownership/**` | 2 | 1,300 | 27 | 96 | 91 |
| `ast/postgres.rs` and `ast/postgres/**` | 4 | 5,520 | 32 | 67 | 51 |
| `ast/sqlite.rs` and `ast/sqlite/**` | 4 | 2,661 | 5 | 6 | 31 |
| `ast/tests.rs` and `ast/tests/**` | 3 | 795 | 29 | 164 | 60 |
| `ast/wildcard.rs` and `ast/wildcard/**` | 4 | 1,351 | 20 | 52 | 57 |
| **Total** | **106** | **51,634** | **598** | **2,196** | **1,861** |

The 598 attributed functions carry 295 `#[test]` and 303 `#[rstest::rstest]`
attributes. The same literal regex
(`#\[\s*case(?:::([A-Za-z_]\w*))?(?=\s|\(|\])`) is applied to each frozen file
and summed per-file and per-area; both sum to 2,196 textual `#[case]` /
`#[case::name]` attributes, including case-row definitions and parameter
markers. `#[values]` appears 25 times. Assertion counts below count direct
assertion macros inside test-attributed functions; the document records counts,
not assertion text.

The earlier implementation-plan sizing was preliminary. The current frozen base
has seven DDL files (including #997's CHECK changes) and 4,059 lines; the
resolver family has 42 files and 15,511 lines.

## Final ownership and API decisions

- The current `ast.rs` tree contains no direct semantic dependency. A source
  scan for `crate::sem`, `sem::`, `SemAst`, `ParseContext`,
  `ScalarAnalysisContext`, and AST methods named `analyze`, `resolve`, or
  `rewrite_wildcards` returned no hits. AST keeps dialect decoding into the
  common AST, inspection, mutation, traversal, structural validation, and
  deparse.
- Semantic conversion is implemented in sem-owned `AstConversion<'a, T>`
  components that borrow common-AST nodes. Statement dispatch and catalog
  application live in `sem/statement_conversion.rs`; raw PostgreSQL/SQLite
  parser entrypoints live in `sem/top_level_statement.rs`. Expression, name,
  query, DML, MERGE, and rule/trigger conversions live in
  `sem/ast_conversion.rs` and its children. DDL conversion is in
  `sem/ddl_conversion.rs` and its children. Ownership, view-lineage, and
  engine/version checks are in `sem/ownership_conversion.rs`,
  `sem/view_conversion.rs`, and `sem/engine_features.rs`.
- `Statement::analyze` was removed from `ast::Statement`. Its sem-owned
  replacement is public `SemAst::from_ast(&Statement, context, &mut
  CacheContext)`. It analyzes an already edited common AST without reparsing and
  keeps caller-owned cache reuse. `SemAst::semantic_analysis` remains the
  parser-input entrypoint. The server now calls `SemAst::from_ast` directly.
- Wildcard rewriting is a crate-root consumer: `rewrite_wildcards(&mut
  Statement, &[WildcardExpansion])` is exported from `lib.rs`, with public
  `WildcardRewriteError`. It consumes semantic expansions and coordinates
  syntax-only AST edit operations in `ast/projection.rs`,
  `ast/document/projection_walk.rs`, and `wildcard_rewrite/**`. It stages edits
  on a clone and commits only after location and expression checks succeed. The
  server's call is now the root function; no semantic expansion interpretation
  remains on `Statement`.
- Shared `AnalysisError` is defined in neutral `src/error.rs`; semantic-only
  helpers remain under `sem/error.rs`, which re-exports `AnalysisError`. Syntax
  conversions such as `drop_behavior.rs` use the neutral definition. This
  preserves a common error type without an AST-to-sem import.
- `syntax_options.rs` is a crate-private neutral home for syntax-valued option
  enums shared by syntax decoding and semantic definitions (including
  generated-column mode, identity mode, foreign-key actions/match type, and
  deferrability). It contains no catalog lookup or semantic decision service.
- SERIAL keeps its pure AST helper in `ast/ddl/serial.rs`: it clones a column
  node and replaces the SERIAL spelling with its integer type.
  `sem/ddl_conversion/serial.rs` calls it for a temporary node, resolves that
  temporary into a semantic column, validates conflicts and stages
  `CreateSequence` catalog effects. The expanded syntax is not written back into
  the user's AST or retained as semantic state.
- `ast/validation.rs` and `ast/document_validation.rs` retain structural syntax
  validation and use the crate stack operation directly.
  `ast/default_privileges.rs` and `ast/foreign_table.rs` retain syntax only;
  their analysis branches remain the documented TODOs #955 and #956 in sem
  dispatch. No SQL support was added by this ownership move.

The source paths and baseline test counts below cover all 106 old Rust files.
`case attrs` is the sum of textual case-row definitions and parameter markers;
assertion counts are direct assertion macros inside test-attributed functions.

| Baseline source file | Test funcs / case attrs / assertions | Final destination and retained responsibility |
| --- | ---: | --- |
| `ast.rs` | 0 / 0 / 0 | AST node definitions remain; analysis dispatch is in `sem/statement_conversion.rs` and raw parser entry points in `sem/top_level_statement.rs`. |
| `ast/ddl.rs` | 6 / 23 / 22 | AST syntax declarations remain here; semantic DDL conversion → `sem/ddl_conversion.rs`. |
| `ast/ddl/alter_index.rs` | 0 / 0 / 0 | Semantic conversion/tests → `sem/ddl_conversion/alter_index.rs`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/ddl/check.rs` | 0 / 0 / 0 | AST syntax helper remains here; semantic conversion → `sem/ddl_conversion/check.rs`. |
| `ast/ddl/column_attributes.rs` | 1 / 0 / 9 | AST syntax helper remains here; semantic conversion → `sem/ddl_conversion/column_attributes.rs`. |
| `ast/ddl/column_rename.rs` | 1 / 0 / 3 | Semantic conversion/tests → `sem/ddl_conversion/column_rename.rs`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/ddl/serial.rs` | 5 / 24 / 20 | AST syntax helper remains here; semantic conversion → `sem/ddl_conversion/serial.rs`. |
| `ast/ddl/table_rename.rs` | 1 / 0 / 5 | Semantic conversion/tests → `sem/ddl_conversion/table_rename.rs`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/default_privileges.rs` | 2 / 13 / 12 | Default-privileges syntax remains; interpretation stays explicitly unimplemented in `sem/statement_conversion.rs` (#955). |
| `ast/deparse.rs` | 5 / 0 / 15 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/constraints.rs` | 14 / 26 / 18 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/ddl.rs` | 5 / 73 / 9 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/dml.rs` | 1 / 6 / 0 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/expression.rs` | 3 / 54 / 3 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/lexical.rs` | 20 / 79 / 37 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/query.rs` | 2 / 36 / 2 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/simple_tests.rs` | 1 / 65 / 0 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/statement.rs` | 0 / 0 / 0 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/statement_simple.rs` | 3 / 0 / 1 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/deparse/tests.rs` | 6 / 61 / 7 | SQL deparse and its syntax tests remain at the same AST path. |
| `ast/document.rs` | 0 / 0 / 0 | Common document syntax remains; semantic statement conversion is in `sem/ast_conversion/document.rs`. |
| `ast/document/deparse.rs` | 0 / 0 / 0 | Syntax deparse remains at the same AST path. |
| `ast/document/merge.rs` | 0 / 0 / 0 | MERGE analysis → `sem/ast_conversion/merge.rs`; common AST document syntax remains in `ast/document.rs`. |
| `ast/document/merge/tests.rs` | 3 / 29 / 9 | MERGE tests → `sem/ast_conversion/merge/tests.rs`. |
| `ast/document/merge/tests/assignments.rs` | 10 / 33 / 28 | MERGE tests → `sem/ast_conversion/merge/tests/assignments.rs`. |
| `ast/document/merge/tests/integration.rs` | 8 / 0 / 21 | MERGE tests → `sem/ast_conversion/merge/tests/integration.rs`. |
| `ast/document/merge/tests/views.rs` | 5 / 0 / 15 | MERGE tests → `sem/ast_conversion/merge/tests/views.rs`. |
| `ast/document/projection_walk.rs` | 0 / 0 / 0 | Syntax-only projection traversal remains; crate-root wildcard rewrite consumes it. |
| `ast/document_validation.rs` | 2 / 4 / 2 | Structural AST validation remains at the same path. |
| `ast/drop_behavior.rs` | 0 / 0 / 0 | Syntax-only conversion remains; shared error is now `crate::error::AnalysisError`. |
| `ast/engine_features.rs` | 5 / 24 / 6 | Syntax markers remain here; semantic version/applicability checks → `sem/engine_features.rs`. |
| `ast/foreign_table.rs` | 2 / 12 / 12 | Foreign-table syntax remains; interpretation stays explicitly unimplemented in `sem/statement_conversion.rs` (#956). |
| `ast/mutation.rs` | 0 / 0 / 0 | Public common-AST mutation operations remain at the same path. |
| `ast/ownership.rs` | 0 / 0 / 0 | Ownership syntax remains here; catalog lookup/application → `sem/ownership_conversion.rs`. |
| `ast/ownership/tests.rs` | 27 / 96 / 91 | Colocated tests → `sem/ownership_conversion.rs` test module. |
| `ast/postgres.rs` | 32 / 67 / 51 | PostgreSQL syntax conversion remains; four `test_name_from_range_var` cases → `ast_integration_tests`. |
| `ast/postgres/ddl.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing → `sem/ddl_conversion.rs`. |
| `ast/postgres/document.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing is owned by the corresponding `sem/ast_conversion` family. |
| `ast/postgres/query.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing → `sem/ast_conversion/query.rs`. |
| `ast/projection.rs` | 0 / 0 / 0 | Syntax-only projection editing remains; crate-root wildcard rewrite uses it. |
| `ast/public.rs` | 18 / 30 / 52 | AST inspection API remains; `Statement::analyze` and its tests moved to `SemAst::from_ast` in `sem/statement_conversion.rs`. |
| `ast/query_construction.rs` | 0 / 0 / 0 | Common-AST query construction remains at the same path. |
| `ast/resolve.rs` | 11 / 46 / 41 | Semantic implementation and tests → `sem/ast_conversion.rs`. |
| `ast/resolve/delete.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/delete.rs`. |
| `ast/resolve/delete/tests.rs` | 17 / 40 / 49 | Semantic implementation and tests → `sem/ast_conversion/delete/tests.rs`. |
| `ast/resolve/delete/tests/cursors.rs` | 5 / 17 / 20 | Semantic implementation and tests → `sem/ast_conversion/delete/tests/cursors.rs`. |
| `ast/resolve/delete/tests/sql_driven.rs` | 1 / 0 / 13 | Semantic implementation and tests → `sem/ast_conversion/delete/tests/sql_driven.rs`. |
| `ast/resolve/delete/tests/versions.rs` | 3 / 10 / 9 | Semantic implementation and tests → `sem/ast_conversion/delete/tests/versions.rs`. |
| `ast/resolve/delete/tests/views.rs` | 1 / 8 / 4 | Semantic implementation and tests → `sem/ast_conversion/delete/tests/views.rs`. |
| `ast/resolve/document.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/document.rs`. |
| `ast/resolve/drop_rule.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/drop_rule.rs`. |
| `ast/resolve/drop_rule/tests.rs` | 7 / 17 / 32 | Semantic implementation and tests → `sem/ast_conversion/drop_rule/tests.rs`. |
| `ast/resolve/drop_trigger.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/drop_trigger.rs`. |
| `ast/resolve/drop_trigger/tests.rs` | 6 / 11 / 28 | Semantic implementation and tests → `sem/ast_conversion/drop_trigger/tests.rs`. |
| `ast/resolve/drop_trigger/tests/sqlite.rs` | 5 / 9 / 31 | Semantic implementation and tests → `sem/ast_conversion/drop_trigger/tests/sqlite.rs`. |
| `ast/resolve/insert.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/insert.rs`. |
| `ast/resolve/insert/tests.rs` | 27 / 85 / 90 | Semantic implementation and tests → `sem/ast_conversion/insert/tests.rs`. |
| `ast/resolve/insert/tests/join_and_key_semantics.rs` | 2 / 15 / 11 | Semantic implementation and tests → `sem/ast_conversion/insert/tests/join_and_key_semantics.rs`. |
| `ast/resolve/insert/tests/views.rs` | 6 / 35 / 18 | Semantic implementation and tests → `sem/ast_conversion/insert/tests/views.rs`. |
| `ast/resolve/placement.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/placement.rs`. |
| `ast/resolve/query.rs` | 1 / 6 / 2 | Semantic implementation and tests → `sem/ast_conversion/query.rs`. |
| `ast/resolve/query/clauses.rs` | 1 / 0 / 3 | Semantic implementation and tests → `sem/ast_conversion/query/clauses.rs`. |
| `ast/resolve/query/grouping_tests.rs` | 10 / 76 / 42 | Semantic implementation and tests → `sem/ast_conversion/query/grouping_tests.rs`. |
| `ast/resolve/query/tests.rs` | 22 / 73 / 73 | Semantic implementation and tests → `sem/ast_conversion/query/tests.rs`. |
| `ast/resolve/query/tests/array_subquery.rs` | 10 / 27 / 35 | Semantic implementation and tests → `sem/ast_conversion/query/tests/array_subquery.rs`. |
| `ast/resolve/query/tests/call_arguments.rs` | 11 / 34 / 48 | Semantic implementation and tests → `sem/ast_conversion/query/tests/call_arguments.rs`. |
| `ast/resolve/query/tests/column_scope.rs` | 8 / 33 / 23 | Semantic implementation and tests → `sem/ast_conversion/query/tests/column_scope.rs`. |
| `ast/resolve/query/tests/derived_aliases.rs` | 4 / 18 / 17 | Semantic implementation and tests → `sem/ast_conversion/query/tests/derived_aliases.rs`. |
| `ast/resolve/query/tests/exists.rs` | 6 / 33 / 30 | Semantic implementation and tests → `sem/ast_conversion/query/tests/exists.rs`. |
| `ast/resolve/query/tests/rows.rs` | 6 / 29 / 28 | Semantic implementation and tests → `sem/ast_conversion/query/tests/rows.rs`. |
| `ast/resolve/query/tests/source_namespace.rs` | 22 / 97 / 69 | Semantic implementation and tests → `sem/ast_conversion/query/tests/source_namespace.rs`. |
| `ast/resolve/query/tests/table_functions.rs` | 21 / 77 / 69 | Semantic implementation and tests → `sem/ast_conversion/query/tests/table_functions.rs`. |
| `ast/resolve/query/tests/whole_rows.rs` | 26 / 69 / 90 | Semantic implementation and tests → `sem/ast_conversion/query/tests/whole_rows.rs`. |
| `ast/resolve/query/wildcard_tests.rs` | 21 / 54 / 71 | Semantic implementation and tests → `sem/ast_conversion/query/wildcard_tests.rs`. |
| `ast/resolve/returning.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/returning.rs`. |
| `ast/resolve/rule.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/rule.rs`. |
| `ast/resolve/rule/tests.rs` | 13 / 60 / 49 | Semantic implementation and tests → `sem/ast_conversion/rule/tests.rs`. |
| `ast/resolve/trigger.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/trigger.rs`. |
| `ast/resolve/trigger/tests.rs` | 16 / 59 / 61 | Semantic implementation and tests → `sem/ast_conversion/trigger/tests.rs`. |
| `ast/resolve/update.rs` | 0 / 0 / 0 | Semantic implementation and tests → `sem/ast_conversion/update.rs`. |
| `ast/resolve/update/tests.rs` | 13 / 40 / 39 | Semantic implementation and tests → `sem/ast_conversion/update/tests.rs`. |
| `ast/resolve/update/tests/assignments.rs` | 1 / 0 / 2 | Semantic implementation and tests → `sem/ast_conversion/update/tests/assignments.rs`. |
| `ast/resolve/update/tests/cursors.rs` | 4 / 17 / 16 | Semantic implementation and tests → `sem/ast_conversion/update/tests/cursors.rs`. |
| `ast/resolve/update/tests/views.rs` | 1 / 9 / 2 | Semantic implementation and tests → `sem/ast_conversion/update/tests/views.rs`. |
| `ast/returning_options.rs` | 0 / 0 / 0 | Syntax options remain; RETURNING analysis → `sem/ast_conversion/returning.rs`. |
| `ast/rows_from.rs` | 0 / 0 / 0 | Syntax constructors remain; query/FROM analysis → `sem/ast_conversion/query.rs`. |
| `ast/rule.rs` | 17 / 42 / 32 | Rule syntax construction remains; semantic conversion → `sem/ast_conversion/rule.rs`. |
| `ast/rule/tests.rs` | 6 / 11 / 14 | Dialect-syntax conversion tests remain with their AST implementation. |
| `ast/sqlite.rs` | 1 / 6 / 4 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `sem/ast_conversion`. |
| `ast/sqlite/ddl.rs` | 0 / 0 / 0 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `sem/ast_conversion`. |
| `ast/sqlite/dml.rs` | 4 / 0 / 27 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `sem/ast_conversion`. |
| `ast/sqlite/query.rs` | 0 / 0 / 0 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `sem/ast_conversion`. |
| `ast/sqlite_retention.rs` | 2 / 3 / 4 | SQLite syntax-retention logic remains at the same AST path. |
| `ast/tests.rs` | 24 / 109 / 51 | Cross-component tests → crate-root `ast_integration_tests.rs`. |
| `ast/tests/sqlite_from_groups.rs` | 3 / 37 / 9 | Cross-component tests → crate-root `ast_integration_tests/sqlite_from_groups.rs`. |
| `ast/tests/stack_regressions.rs` | 2 / 18 / 0 | Cross-component tests → crate-root `ast_integration_tests/stack_regressions.rs`. |
| `ast/traversal.rs` | 1 / 0 / 2 | Syntax-tree traversal remains at the same path. |
| `ast/trigger.rs` | 16 / 50 / 54 | Trigger syntax construction remains; semantic conversion → `sem/ast_conversion/trigger.rs`. |
| `ast/trigger/tests.rs` | 3 / 5 / 33 | Dialect-syntax conversion tests remain with their AST implementation. |
| `ast/utility.rs` | 2 / 0 / 8 | Syntax-only AST utility operations remain at the same path. |
| `ast/validation.rs` | 1 / 4 / 1 | Structural AST validation remains; stack handling calls the crate stack directly. |
| `ast/view_definition.rs` | 0 / 0 / 0 | View lineage and semantic conversion → `sem/view_conversion.rs`; source file removed. |
| `ast/wildcard.rs` | 0 / 0 / 0 | Syntax projection operations remain here; semantic expansion consumer → crate-root `wildcard_rewrite.rs`. |
| `ast/wildcard/rows_source.rs` | 4 / 6 / 13 | Rewrite consumer and tests → crate-root `wildcard_rewrite/rows_source.rs`. |
| `ast/wildcard/source.rs` | 0 / 0 / 0 | Rewrite consumer and tests → crate-root `wildcard_rewrite/source.rs`. |
| `ast/wildcard/tests.rs` | 16 / 46 / 44 | Rewrite consumer and tests → crate-root `wildcard_rewrite/tests.rs`. |

## Compiled-case preservation comparison

The final all-features test-list comparison covers 28 `sql-fun-sqlast` suites
and all 5,108 compiled cases. It maps baseline identities to final identities by
exact equality first, then by the 11 explicit prefixes below while preserving
the entire remaining suffix (including each parameterized case identity). The
normalized identity digest is equal before and after:
`e6d0734e5352eb2c2ee2afc728583da4d066a9da5a5b5c9ec60c821a0f8860d0`.

| Old identity prefix | Final identity prefix | Cases |
| --- | --- | ---: |
| `ast::resolve::` | `sem::ast_conversion::` | 1,092 |
| `ast::document::merge::` | `sem::ast_conversion::merge::` | 64 |
| `ast::ddl::` | `sem::ddl_conversion::` | 36 |
| `ast::ownership::` | `sem::ownership_conversion::` | 73 |
| `ast::public::` | `sem::statement_conversion::` | 31 |
| `ast::engine_features::` | `sem::engine_features::` | 24 |
| `ast::tests::` | `ast_integration_tests::` | 174 |
| `ast::postgres::test_name_from_range_var::` | `ast_integration_tests::test_name_from_range_var::` | 4 |
| `ast::wildcard::` | `wildcard_rewrite::` | 57 |
| `sem::column_constraints::generated_constraint::test_generated_column_mode_from_pg::` | `syntax_options::test_generated_column_mode_from_pg::` | 3 |
| `sem::column_constraints::identity_constraint::identity_column_mode_from_postgres_generate_when::` | `syntax_options::identity_column_mode_from_postgres_generate_when::` | 3 |
| **Total moved cases** |  | **1,561** |

Of 5,108 baseline all-features cases, 3,547 keep their exact suite/test identity
and 1,561 move under those prefixes. Final all-features has the same 5,108 cases
across the same 28 suites; there are no unmatched baseline identities,
unexpected final identities, mapping collisions, or changes to case metadata
(`kind`, `ignored`, `filter-match`). Syntax-only cases retain their `ast::`
names through the exact-equality-first mapping; the migration does not rewrite
the whole namespace.

The default-profile identity comparison also reconciles exactly: 4,381 baseline
cases to 4,381 final cases across the same 28 suites, with 3,051 unchanged and
1,330 moved under the explicit identity prefixes. It found no unmatched baseline
identities, unexpected final identities, mapping collisions, or case-metadata
changes. The normalized default identity digest is
`2a16670a1a022c8cbf5ea785c715a2efc411d2a7a6ccf96af6cba6b0a21b4e31`. The two
default-only `cfg(not(feature = "sqlite"))` cases remain in the final default
profile; their source files are unchanged from baseline and they are
intentionally absent from all-features inventories.

### Source assertion and case-count evidence

The resolver and MERGE source ledger covers 47 old files: 334 attributed test
functions, 1,166 `#[case]` attributes, and 1,188 assertion macros inside those
attributed test bodies. A whole-file assertion-macro regex yields 1,198 hits;
the extra 10 are in `ast/resolve/query/tests/column_scope.rs` (3),
`ast/resolve/query/tests/exists.rs` (1), and
`ast/resolve/query/wildcard_tests.rs` (6), outside the attributed function
bodies. Its compiled cases are included in the all-features identity
reconciliation above (1,092 resolver and 64 MERGE cases). The destination
mapping is path-preserving from `ast/resolve[/...]` to
`sem/ast_conversion[/...]`, with the explicit `ast/document/merge[/...]` to
`sem/ast_conversion/merge[/...]` rule.

The DDL/ownership/view/engine ledger covers 11 old files. Its before/after
source counts match: 21 `#[test]`, 25 `#[rstest::rstest]`, 167 `#[case]`, two
`#[should_panic]`, and 156 assertion macros. The 73 ownership cases are inlined
under `sem/ownership_conversion::tests`; CHECK and SERIAL syntax helpers remain
in AST while their semantic conversion and catalog effects are in sem.

The API migrations intentionally change caller expressions
(`statement.analyze(...)` to `SemAst::from_ast(...)`, and the `Statement`
wildcard method to crate-root `rewrite_wildcards(...)`). Therefore this ledger
claims compiled identity and metadata preservation plus the DDL source
assertion-count check above; it does not claim byte-for-byte equality of
assertion expressions or that calls were left unchanged.

## Existing consumers and regression inventory

- The server analyzes edited statements through `SemAst::from_ast` in
  `sql-fun-server/src/session/query_analysis.rs` and applies wildcard expansions
  through `rewrite_wildcards`. Its cache context remains caller-owned and is
  reused across statements.
- Existing direct-analysis examples are colocated in
  `sem/statement_conversion.rs`; callers also exist in parameter-resolution
  tests and crate `test_helpers`. No semantic analysis example remains attached
  to the AST `Statement` API.
- The SQLAST CLI regression manifest at the base has 85 fixture rows expanding
  to 166 fixture/version groups. Every manifest case count matches its JSON
  fixture length. Manifest-derived execution counts are 1,542 inputs for
  PostgreSQL 17 and 1,522 for PostgreSQL 18. The README at that same revision
  still states 1,214 and 1,197 (2,411 total); this inventory records the
  discrepancy without changing unrelated documentation.

## Validation status and limits

Both default and all-features compiled lists reconcile exactly as recorded
above. The full `.local/bin/check-test-flow --all-features` completed
successfully. Default execution passed 4,381 SQLAST cases and 5,475 workspace
cases; all-features execution passed 5,108 SQLAST cases and 6,254 workspace
cases (63 existing ignored workspace cases in either configuration). The
PostgreSQL 17/18 CLI regression suite passed all 3,064 inputs in 166 groups in
both configurations, including expected-failure controls. Feature-boundary
checks, Clippy, Markdown, rustdoc, manual generation and doctests also passed.
No case deletion or timeout-policy exception was needed.

Distribution and SemVer results are recorded in the PR validation report.
Source counts and compiled identities alone do not establish unchanged runtime
behavior, diagnostics, source spans or catalog effects; the existing execution
suites and independent architecture review provide additional evidence.

## Final semantic entry-point index

Each row identifies the consuming `AstConversion<AST type>` implementation and
its operations. The file ledger above identifies its old source; helper
extraction may add focused operations without adding SQL behavior. This adapter
is sem-owned, borrows the syntax node, and returns existing semantic
representations rather than a duplicate AST. `SemAst::from_ast` is the public
statement entry point.

| Semantic owner file | Borrowed AST type | Operations |
| --- | --- | --- |
| `sem/ast_conversion.rs` | `Access` | `is_subscript`, `resolve`, `resolve_sequence`, `resolve_subscript`, `subscript_prefix_length` |
| `sem/ast_conversion.rs` | `BooleanExpr` | `resolve` |
| `sem/ast_conversion.rs` | `BooleanOperator` | `resolve`, `resolve_not` |
| `sem/ast_conversion.rs` | `Call` | `resolve`, `resolve_coalesce`, `resolve_min_max`, `resolve_ordinary`, `resolve_supplied_call`, `uses_supplied_binding` |
| `sem/ast_conversion.rs` | `CallKind` | `is_ordinary`, `resolve` |
| `sem/ast_conversion.rs` | `Case` | `resolve` |
| `sem/ast_conversion.rs` | `Cast` | `resolve` |
| `sem/ast_conversion.rs` | `Collate` | `resolve` |
| `sem/ast_conversion.rs` | `Expr` | `invalid_star_projection`, `is_named_argument`, `negated_literal`, `projection_base`, `resolve`, `resolve_array`, `resolve_array_target`, `resolve_grouping`, `resolve_in`, `resolve_in_body`, `resolve_list`, `resolve_merge_action`, `resolve_optional`, `resolve_parenthesized`, `resolve_row`, `resolve_scalar_in`, `resolve_scalar_list` |
| `sem/ast_conversion.rs` | `Indirection` | `projection_base`, `resolve` |
| `sem/ast_conversion.rs` | `Literal` | `negate`, `resolve` |
| `sem/ast_conversion.rs` | `Name` | `append_row_projection`, `full_name`, `operator_name`, `optional_full_name`, `resolve_column`, `star_qualifier`, `star_relation` |
| `sem/ast_conversion.rs` | `NamedArgument` | `resolve` |
| `sem/ast_conversion.rs` | `NullTest` | `resolve` |
| `sem/ast_conversion.rs` | `OperatorExpr` | `distinctness`, `resolve`, `resolve_operator`, `resolve_sqlite_pattern` |
| `sem/ast_conversion.rs` | `OperatorKind` | `distinctness`, `has_named_operator`, `is_not_like`, `is_pattern`, `membership_negated`, `resolve`, `resolve_between`, `resolve_operator_name`, `resolve_symmetric_between`, `without_catalog_operator` |
| `sem/ast_conversion.rs` | `SqlValue` | `resolve` |
| `sem/ast_conversion.rs` | `Subquery` | `resolve` |
| `sem/ast_conversion.rs` | `SubqueryKind` | `is_membership`, `resolve` |
| `sem/ast_conversion.rs` | `TruthTest` | `resolve` |
| `sem/ast_conversion.rs` | `TypeName` | `declaration_type`, `modifier_arguments`, `reference` |
| `sem/ast_conversion/delete.rs` | `CurrentOf` | `resolve_delete` |
| `sem/ast_conversion/delete.rs` | `Delete` | `resolve`, `resolve_destination`, `resolve_in`, `resolve_predicate`, `resolve_returning_scope`, `resolve_sources`, `validate_index_hint` |
| `sem/ast_conversion/delete.rs` | `Expr` | `resolve_delete_current_of` |
| `sem/ast_conversion/document.rs` | `XmlTable` | `resolve` |
| `sem/ast_conversion/drop_rule.rs` | `DropRule` | `resolve`, `resolve_and_apply`, `resolve_definition`, `resolve_on` |
| `sem/ast_conversion/drop_trigger.rs` | `DropBehavior` | `semantic` |
| `sem/ast_conversion/drop_trigger.rs` | `DropTrigger` | `resolve`, `resolve_and_apply`, `resolve_on`, `resolve_postgres`, `resolve_sqlite` |
| `sem/ast_conversion/insert.rs` | `Assignment` | `overlaps_mapped` |
| `sem/ast_conversion/insert.rs` | `Collate` | `collect_insert_collations` |
| `sem/ast_conversion/insert.rs` | `Conflict` | `require_insert_action`, `resolve_insert` |
| `sem/ast_conversion/insert.rs` | `ConflictAction` | `require_insert_action` |
| `sem/ast_conversion/insert.rs` | `Expr` | `collect_insert_collations`, `collect_parenthesized_insert_collations`, `resolve_insert_inference`, `resolve_insert_value`, `resolve_parenthesized_insert_inference` |
| `sem/ast_conversion/insert.rs` | `IndexExpression` | `resolve_insert_modifiers` |
| `sem/ast_conversion/insert.rs` | `Insert` | `resolve`, `resolve_definition`, `resolve_in`, `validate_expanded_source` |
| `sem/ast_conversion/insert.rs` | `OverridingKind` | `insert_mode`, `overrides_identity` |
| `sem/ast_conversion/insert.rs` | `Query` | `resolve_insert_source` |
| `sem/ast_conversion/merge.rs` | `Expr` | `resolve_merge_predicate` |
| `sem/ast_conversion/merge.rs` | `Merge` | `require_version`, `resolve`, `resolve_clauses`, `resolve_in` |
| `sem/ast_conversion/merge.rs` | `MergeMatchKind` | `index`, `permits`, `resolve` |
| `sem/ast_conversion/merge.rs` | `MergeWhen` | `resolve_action`, `resolve_insert_action`, `resolve_update_action` |
| `sem/ast_conversion/merge.rs` | `OverridingKind` | `merge_override` |
| `sem/ast_conversion/placement.rs` | `CteBody` | `require_data_modifying_placement` |
| `sem/ast_conversion/query.rs` | `Alias` | `resolve` |
| `sem/ast_conversion/query.rs` | `Call` | `resolve_modifiers`, `resolve_table`, `table_output_name` |
| `sem/ast_conversion/query.rs` | `Cast` | `table_output_name` |
| `sem/ast_conversion/query.rs` | `CteBody` | `resolve` |
| `sem/ast_conversion/query.rs` | `Delete` | `resolve_cte_body` |
| `sem/ast_conversion/query.rs` | `DerivedTable` | `resolve` |
| `sem/ast_conversion/query.rs` | `Expr` | `resolve_expression_table`, `resolve_from_expression`, `resolve_rows`, `resolve_rows_call`, `resolve_table_call`, `table_output_name` |
| `sem/ast_conversion/query.rs` | `FromGroup` | `resolve` |
| `sem/ast_conversion/query.rs` | `FromItem` | `resolve`, `resolve_body`, `resolve_in_scope`, `resolve_list`, `resolve_list_in`, `resolve_list_in_scope`, `resolve_source` |
| `sem/ast_conversion/query.rs` | `Insert` | `resolve_cte_body` |
| `sem/ast_conversion/query.rs` | `Into` | `resolve_destination` |
| `sem/ast_conversion/query.rs` | `Join` | `resolve`, `using_clause` |
| `sem/ast_conversion/query.rs` | `JoinKind` | `resolve` |
| `sem/ast_conversion/query.rs` | `Merge` | `resolve_cte_body` |
| `sem/ast_conversion/query.rs` | `Name` | `table_output_name` |
| `sem/ast_conversion/query.rs` | `OperatorExpr` | `table_output_name` |
| `sem/ast_conversion/query.rs` | `OperatorKind` | `is_null_if` |
| `sem/ast_conversion/query.rs` | `Query` | `resolve`, `resolve_cte_body`, `resolve_in`, `resolve_in_body`, `resolve_query_body`, `resolve_test_in` |
| `sem/ast_conversion/query.rs` | `QueryBody` | `is_compound`, `order_binding`, `resolve`, `resolve_values`, `set_all`, `window_names` |
| `sem/ast_conversion/query.rs` | `Relation` | `resolve` |
| `sem/ast_conversion/query.rs` | `Select` | `build_result`, `resolve`, `resolve_clauses`, `resolve_distinct`, `resolve_from`, `resolve_grouping`, `resolve_having`, `resolve_targets`, `resolve_where`, `resolve_windows`, `scope`, `validate_rule_destination` |
| `sem/ast_conversion/query.rs` | `SetOperation` | `resolve` |
| `sem/ast_conversion/query.rs` | `SetQuery` | `resolve` |
| `sem/ast_conversion/query.rs` | `SqlValue` | `table_output_name` |
| `sem/ast_conversion/query.rs` | `TableFunctions` | `resolve`, `resolve_rows_from` |
| `sem/ast_conversion/query.rs` | `TableSample` | `resolve` |
| `sem/ast_conversion/query.rs` | `Target` | `append_source_star`, `append_to_results` |
| `sem/ast_conversion/query.rs` | `Update` | `resolve_cte_body` |
| `sem/ast_conversion/query.rs` | `With` | `local_definitions`, `resolve`, `resolve_with` |
| `sem/ast_conversion/query/clauses.rs` | `Expr` | `ordinal`, `resolve_clause`, `sqlite_comparison_sql`, `unqualified_column` |
| `sem/ast_conversion/query/clauses.rs` | `Frame` | `resolve` |
| `sem/ast_conversion/query/clauses.rs` | `FrameBound` | `resolve` |
| `sem/ast_conversion/query/clauses.rs` | `FrameExclusion` | `semantic_name` |
| `sem/ast_conversion/query/clauses.rs` | `FrameMode` | `semantic_name` |
| `sem/ast_conversion/query/clauses.rs` | `Grouping` | `resolve`, `resolve_expression`, `resolve_list` |
| `sem/ast_conversion/query/clauses.rs` | `Lock` | `resolve_list` |
| `sem/ast_conversion/query/clauses.rs` | `LockStrength` | `semantic_name` |
| `sem/ast_conversion/query/clauses.rs` | `LockWait` | `semantic_name` |
| `sem/ast_conversion/query/clauses.rs` | `Name` | `unqualified` |
| `sem/ast_conversion/query/clauses.rs` | `Order` | `checked_name`, `resolve`, `resolve_for_query`, `resolve_list`, `resolve_using_operator` |
| `sem/ast_conversion/query/clauses.rs` | `QueryBody` | `match_sqlite_order`, `match_sqlite_values_order` |
| `sem/ast_conversion/query/clauses.rs` | `Select` | `match_sqlite_order` |
| `sem/ast_conversion/query/clauses.rs` | `SetQuery` | `match_sqlite_order` |
| `sem/ast_conversion/query/clauses.rs` | `Window` | `names`, `resolve`, `resolve_definitions`, `resolve_over` |
| `sem/ast_conversion/returning.rs` | `Name` | `require_returning_star` |
| `sem/ast_conversion/rule.rs` | `CreateRule` | `check_columns`, `invalid`, `resolve`, `resolve_and_apply`, `resolve_definition`, `resolve_rule_predicate`, `validate_resolved`, `validate_results`, `validate_shape` |
| `sem/ast_conversion/rule.rs` | `CteBody` | `validate_select_rule_ctes` |
| `sem/ast_conversion/rule.rs` | `Expr` | `is_rule_boolean_literal` |
| `sem/ast_conversion/rule.rs` | `Literal` | `is_rule_boolean_literal` |
| `sem/ast_conversion/rule.rs` | `Query` | `validate_select_rule_ctes` |
| `sem/ast_conversion/rule.rs` | `RuleEvent` | `semantic` |
| `sem/ast_conversion/rule.rs` | `StatementNode` | `resolve_rule_action`, `rule_query`, `validate_select_rule_ctes` |
| `sem/ast_conversion/rule.rs` | `SubqueryKind` | `pending_rule_semantics` |
| `sem/ast_conversion/trigger.rs` | `CreateTrigger` | `analysis_scope`, `finish_definition`, `has_new`, `has_old`, `invalid`, `relation`, `resolve`, `resolve_and_apply`, `resolve_events`, `resolve_identity`, `resolve_in`, `resolve_predicate`, `resolve_row_bindings`, `resolve_target`, `resolve_transitions`, `validate`, `validate_identity` |
| `sem/ast_conversion/trigger.rs` | `Delete` | `resolve_trigger_command` |
| `sem/ast_conversion/trigger.rs` | `Expr` | `untyped_trigger_predicate` |
| `sem/ast_conversion/trigger.rs` | `Insert` | `resolve_trigger_command` |
| `sem/ast_conversion/trigger.rs` | `Literal` | `untyped_trigger_predicate` |
| `sem/ast_conversion/trigger.rs` | `Relation` | `validate_trigger_command` |
| `sem/ast_conversion/trigger.rs` | `StatementNode` | `resolve_trigger_command` |
| `sem/ast_conversion/trigger.rs` | `TriggerBody` | `resolve` |
| `sem/ast_conversion/trigger.rs` | `TriggerConstraint` | `resolve` |
| `sem/ast_conversion/trigger.rs` | `TriggerEvent` | `has_columns`, `has_new`, `has_old`, `is_truncate`, `resolve` |
| `sem/ast_conversion/trigger.rs` | `TriggerFunction` | `resolve` |
| `sem/ast_conversion/trigger.rs` | `TriggerGranularity` | `semantic` |
| `sem/ast_conversion/trigger.rs` | `TriggerTiming` | `semantic` |
| `sem/ast_conversion/trigger.rs` | `Update` | `resolve_trigger_command` |
| `sem/ast_conversion/update.rs` | `Assignment` | `resolve_update_value`, `validate_update_value` |
| `sem/ast_conversion/update.rs` | `CurrentOf` | `resolve_update` |
| `sem/ast_conversion/update.rs` | `Expr` | `resolve_current_of`, `resolve_update_row`, `resolve_update_row_values`, `resolve_update_value` |
| `sem/ast_conversion/update.rs` | `Subquery` | `resolve_update_row` |
| `sem/ast_conversion/update.rs` | `Update` | `resolve`, `resolve_definition`, `resolve_destination`, `resolve_in`, `resolve_predicate`, `resolve_returning_scope`, `resolve_sources`, `validate_index_hint`, `validate_view_trigger` |
| `sem/ddl_conversion.rs` | `AlterCommand` | `check_engine_features`, `existing_column`, `missing_column`, `resolve`, `resolve_add_column`, `resolve_alter_column`, `resolve_and_report`, `resolve_cluster`, `resolve_constraint`, `resolve_default`, `resolve_drop_column`, `resolve_owner`, `resolve_rename`, `resolve_rename_column`, `view_default_action` |
| `sem/ddl_conversion.rs` | `AlterConstraintFlags` | `check_engine_features` |
| `sem/ddl_conversion.rs` | `AlterDefinition` | `check_engine_features`, `view_default` |
| `sem/ddl_conversion.rs` | `AlterRelation` | `resolve_and_apply`, `resolve_and_apply_at`, `resolve_and_apply_with_source`, `resolve_in`, `view_default_changes` |
| `sem/ddl_conversion.rs` | `AlterRelationKind` | `resolve`, `resolve_index`, `resolve_sequence`, `resolve_table`, `resolve_view` |
| `sem/ddl_conversion.rs` | `AstColumn` | `resolve`, `resolve_and_report`, `resolve_constraints`, `resolve_declaration`, `resolve_standalone`, `resolve_type` |
| `sem/ddl_conversion.rs` | `AstColumnConstraint` | `has_catalog_name`, `resolve`, `resolve_and_report`, `resolve_declared_foreign_key`, `resolve_table`, `resolve_table_primary_key`, `resolve_table_unique`, `supports_named_not_null` |
| `sem/ddl_conversion.rs` | `CompositeField` | `resolve` |
| `sem/ddl_conversion.rs` | `CompositeType` | `resolve`, `resolve_and_apply` |
| `sem/ddl_conversion.rs` | `Domain` | `check_engine_features`, `resolve`, `resolve_and_apply`, `resolve_in` |
| `sem/ddl_conversion.rs` | `Expr` | `resolved_view_default` |
| `sem/ddl_conversion.rs` | `ForeignReference` | `referenced_columns`, `report_ignored_matches`, `resolve`, `resolve_table`, `resolve_table_in` |
| `sem/ddl_conversion.rs` | `Function` | `resolve`, `resolve_and_apply`, `resolve_in` |
| `sem/ddl_conversion.rs` | `FunctionParameter` | `resolve`, `resolve_and_report` |
| `sem/ddl_conversion.rs` | `Index` | `resolve`, `resolve_and_apply`, `resolve_elements`, `resolve_elements_in`, `resolve_in` |
| `sem/ddl_conversion.rs` | `ParameterMode` | `resolve_parameter` |
| `sem/ddl_conversion.rs` | `PostgresAlterCommand` | `resolve`, `resolve_owner`, `view_column_default`, `view_default_action` |
| `sem/ddl_conversion.rs` | `Schema` | `resolve_and_apply`, `resolve_group` |
| `sem/ddl_conversion.rs` | `Table` | `constraint_names`, `resolve`, `resolve_and_apply`, `resolve_and_apply_at`, `resolve_foreign_keys`, `resolve_in`, `resolve_with_names` |
| `sem/ddl_conversion.rs` | `TableAs` | `resolve_and_apply`, `resolve_and_apply_at` |
| `sem/ddl_conversion.rs` | `TableConstraint` | `column_names`, `delete_column_names`, `resolve`, `resolve_declared_foreign_key`, `resolve_declared_key` |
| `sem/ddl_conversion/alter_index.rs` | `AlterCommand` | `resolve_index` |
| `sem/ddl_conversion/alter_index.rs` | `AlterDefinition` | `index_statistics_target`, `index_storage_parameters`, `resolve_index_statistics`, `resolve_index_storage` |
| `sem/ddl_conversion/alter_index.rs` | `AlterOperation` | `resolve_index` |
| `sem/ddl_conversion/alter_index.rs` | `Expr` | `index_option_value`, `index_statistics_target` |
| `sem/ddl_conversion/alter_index.rs` | `Index` | `initialize_index_storage` |
| `sem/ddl_conversion/alter_index.rs` | `Literal` | `index_option_value`, `index_statistics_target` |
| `sem/ddl_conversion/alter_index.rs` | `Name` | `index_option_word` |
| `sem/ddl_conversion/alter_index.rs` | `PostgresAlterCommand` | `resolve_index` |
| `sem/ddl_conversion/alter_index.rs` | `SyntaxOption` | `index_storage_parameters` |
| `sem/ddl_conversion/alter_index.rs` | `SyntaxValue` | `index_option_value` |
| `sem/ddl_conversion/alter_index.rs` | `TypeName` | `index_option_word` |
| `sem/ddl_conversion/check.rs` | `Column` | `has_inline_checks`, `reserve_constraint_names`, `resolve_inline_checks` |
| `sem/ddl_conversion/check.rs` | `ColumnConstraint` | `resolve_standalone_check` |
| `sem/ddl_conversion/check.rs` | `Table` | `resolve_checks`, `resolve_inline_checks` |
| `sem/ddl_conversion/check.rs` | `TableConstraint` | `analyze_legacy_check`, `resolve_check`, `resolve_check_expression`, `resolve_legacy_inline_check` |
| `sem/ddl_conversion/column_attributes.rs` | `AlterCommand` | `column_attribute_phase`, `resolve_column_attribute` |
| `sem/ddl_conversion/column_attributes.rs` | `AlterDefinition` | `column_options`, `statistics_request`, `validate_statistics` |
| `sem/ddl_conversion/column_attributes.rs` | `AlterOperation` | `column_attribute_action`, `column_attribute_phase`, `drops_expression`, `drops_not_null`, `sets_statistics` |
| `sem/ddl_conversion/column_attributes.rs` | `AlterRelation` | `has_column_attributes`, `resolve_column_attributes` |
| `sem/ddl_conversion/column_attributes.rs` | `Expr` | `column_option_text`, `statistics_integer` |
| `sem/ddl_conversion/column_attributes.rs` | `Literal` | `column_option_text`, `statistics_integer` |
| `sem/ddl_conversion/column_attributes.rs` | `PostgresAlterCommand` | `resolve_column_attribute`, `resolve_single_column_attribute` |
| `sem/ddl_conversion/column_attributes.rs` | `SyntaxOption` | `column_attribute_option` |
| `sem/ddl_conversion/column_attributes.rs` | `SyntaxValue` | `column_option_text` |
| `sem/ddl_conversion/column_rename.rs` | `AlterCommand` | `column_rename_names`, `is_column_rename` |
| `sem/ddl_conversion/column_rename.rs` | `AlterRelation` | `has_column_rename`, `resolve_column_rename` |
| `sem/ddl_conversion/serial.rs` | `Column` | `resolve_serial`, `serial_integer`, `validate_serial` |
| `sem/ddl_conversion/serial.rs` | `ColumnConstraint` | `serial_conflict` |
| `sem/ddl_conversion/serial.rs` | `TypeName` | `serial_integer` |
| `sem/ddl_conversion/table_rename.rs` | `AlterCommand` | `is_table_rename`, `table_rename_destination` |
| `sem/ddl_conversion/table_rename.rs` | `AlterRelation` | `has_table_rename`, `resolve_table_rename` |
| `sem/engine_features.rs` | `AlterRelation` | `check_engine_features` |
| `sem/engine_features.rs` | `Column` | `check_engine_features` |
| `sem/engine_features.rs` | `ColumnConstraint` | `check_enforcement`, `check_engine_features`, `check_not_null` |
| `sem/engine_features.rs` | `CteBody` | `check_engine_features` |
| `sem/engine_features.rs` | `DefaultPrivileges` | `check_engine_features` |
| `sem/engine_features.rs` | `Delete` | `check_engine_features` |
| `sem/engine_features.rs` | `ForeignTable` | `check_engine_features` |
| `sem/engine_features.rs` | `Insert` | `check_engine_features` |
| `sem/engine_features.rs` | `Merge` | `check_engine_features` |
| `sem/engine_features.rs` | `Query` | `check_engine_features` |
| `sem/engine_features.rs` | `ReturningOption` | `check_features` |
| `sem/engine_features.rs` | `StatementNode` | `check_engine_features` |
| `sem/engine_features.rs` | `Table` | `check_engine_features` |
| `sem/engine_features.rs` | `TableConstraint` | `check_engine_features`, `check_temporal_features` |
| `sem/engine_features.rs` | `Update` | `check_engine_features` |
| `sem/engine_features.rs` | `With` | `check_engine_features` |
| `sem/ownership_conversion.rs` | `AlterOwner` | `resolve`, `resolve_and_apply`, `target_identity` |
| `sem/ownership_conversion.rs` | `Expr` | `owned_identifier`, `owned_relation`, `owned_relation_from_parts` |
| `sem/ownership_conversion.rs` | `Literal` | `owned_identifier`, `owned_relation` |
| `sem/ownership_conversion.rs` | `Name` | `owned_relation` |
| `sem/ownership_conversion.rs` | `ObjectTarget` | `ownership_identity` |
| `sem/ownership_conversion.rs` | `Role` | `resolve_named_owner`, `resolve_owner` |
| `sem/ownership_conversion.rs` | `SyntaxOption` | `owned_relation` |
| `sem/ownership_conversion.rs` | `TypeName` | `owner_argument_type` |
| `sem/statement_conversion.rs` | `AlterSequence` | `apply` |
| `sem/statement_conversion.rs` | `Comment` | `apply` |
| `sem/statement_conversion.rs` | `DropTable` | `apply` |
| `sem/statement_conversion.rs` | `EnumType` | `apply` |
| `sem/statement_conversion.rs` | `Extension` | `apply` |
| `sem/statement_conversion.rs` | `Sequence` | `apply` |
| `sem/statement_conversion.rs` | `StatementNode` | `resolve`, `resolve_and_apply` |
| `sem/statement_conversion.rs` | `VariableSet` | `resolve_and_apply` |
| `sem/statement_conversion.rs` | `View` | `resolve_and_apply` |
| `sem/view_conversion.rs` | `AstViewCheckOption` | `semantic` |
| `sem/view_conversion.rs` | `Cast` | `view_cast_column` |
| `sem/view_conversion.rs` | `Expr` | `view_cast_column` |
| `sem/view_conversion.rs` | `FromItem` | `automatic_view_source` |
| `sem/view_conversion.rs` | `Name` | `view_source_column` |
| `sem/view_conversion.rs` | `Query` | `automatic_view_source`, `refine_view_cast_lineage`, `view_lineage` |
| `sem/view_conversion.rs` | `QueryBody` | `automatic_view_source` |
| `sem/view_conversion.rs` | `View` | `semantic_options` |
