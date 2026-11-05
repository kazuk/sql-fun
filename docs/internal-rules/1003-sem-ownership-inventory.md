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
- Conversion implementations are inherent operations on their semantic result
  types: scalar expressions on `SemScalarExpr`, queries on `SelectStatement`,
  DML on their semantic statement types, and DDL on their semantic definitions.
  Statement dispatch and application belong to `SemAst`; version validation
  and multi-kind ALTER orchestration use concrete semantic processing
  components. Each receives common AST explicitly, alongside the existing
  context. The source-typed generic adapter from the first PR revision is
  removed; it is not retained as a renamed service or compatibility facade.
  The detailed old-operation to destination-owner index below records the
  implementation boundary as well as its physical file location.
- `Statement::analyze` was removed from `ast::Statement`. Its sem-owned
  replacement is public `SemAst::from_ast(&Statement, context, &mut
  CacheContext)`. It analyzes an already edited common AST without reparsing and
  keeps caller-owned cache reuse. `SemAst::semantic_analysis` remains the
  parser-input entrypoint. The server now calls `SemAst::from_ast` directly.
- Wildcard rewriting belongs to the semantic expansion result:
  `WildcardExpansion::rewrite(&[Self], &mut Statement)` coordinates the
  syntax-only edits and owns the staged rewrite plan under
  `sem/wildcard_expansion/rewrite`. `WildcardRewriteError` is exported by
  `sem`. The crate-root rewriting function and exports are removed. All
  lookups and expressions are validated before the staged clone is committed,
  preserving failure atomicity. Syntax-only location synchronization has its
  own `ast::TargetLocationError`, so its API has no semantic dependency.
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
| `ast/document/projection_walk.rs` | 0 / 0 / 0 | Syntax-only projection traversal remains; semantic wildcard rewrite consumes it. |
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
| `ast/projection.rs` | 0 / 0 / 0 | Syntax-only projection editing remains; semantic wildcard rewrite uses it. |
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
| `ast/wildcard.rs` | 0 / 0 / 0 | Syntax projection operations remain here; semantic expansion consumer → `sem/wildcard_expansion/rewrite.rs`. |
| `ast/wildcard/rows_source.rs` | 4 / 6 / 13 | Rewrite consumer and tests → `sem/wildcard_expansion/rewrite/rows_source.rs`. |
| `ast/wildcard/source.rs` | 0 / 0 / 0 | Rewrite consumer and tests → `sem/wildcard_expansion/rewrite/source.rs`. |
| `ast/wildcard/tests.rs` | 16 / 46 / 44 | Rewrite consumer and tests → `sem/wildcard_expansion/rewrite/tests.rs`. |

## Compiled-case preservation status

The captured baseline inventories contain 4,381 default-profile cases and
5,108 all-features cases across 28 `sql-fun-sqlast` suites. A review correction
exposed that a misplaced `cfg(feature = "sqlite")` gated the `cursors` test
modules and hid 12 default-profile cases. The module gate was removed.

The post-fix default comparison against first PR head `22fca6e0` reconciles
4,381 cases across the same 28 suites: 4,324 keep their identity and 57
wildcard-rewrite cases move from `wildcard_rewrite::` under
`sem::wildcard_expansion::rewrite::`. There are no missing or extra identities
or metadata changes; the normalized identity SHA-256 is
`1a32aaa07f384789a3434b032ed44141160cc6159ad7e2b8798f88634b2f5bc7`. The
reconciliation confirms the 12 cursor cases are present after the gate fix.
An interim 4,369-case run before that fix is superseded by this comparison and
the post-fix standard flow.

The post-fix all-features comparison against first PR head `22fca6e0` also
reconciles exactly: 5,108 cases across the same 28 suites, with 5,051 unchanged
and 57 wildcard-rewrite cases relocated from `wildcard_rewrite::` under
`sem::wildcard_expansion::rewrite::`. There are no missing or extra identities
or metadata changes; the normalized identity SHA-256 is
`697af4119273047513f2e4484bbbac5e59878e76b243a55f8b78af6d56d80984`.

Both post-fix standard flows passed. Default passed 4,381 `sql-fun-sqlast`
tests and 5,475 workspace tests with 63 existing ignored; all-features passed
5,108 `sql-fun-sqlast` tests and 6,254 workspace tests with 63 existing
ignored. Each configuration passed 3,064 SQL regression inputs in 166 groups.
Packaging and SemVer checks also passed for implementation commit `7a047313`.
The generated Debian/tar layout, metadata, checksums, and executable identity
were validated. Executables extracted from the tar archive passed all 3,064
SQL regression cases across 166 groups. Existing release-intent declarations
and package versions were unchanged.

### Source assertion and case-count evidence

The resolver and MERGE source ledger covers 47 old files: 334 attributed test
functions, 1,166 `#[case]` attributes, and 1,188 assertion macros inside those
attributed test bodies. A whole-file assertion-macro regex yields 1,198 hits;
the extra 10 are in `ast/resolve/query/tests/column_scope.rs` (3),
`ast/resolve/query/tests/exists.rs` (1), and
`ast/resolve/query/wildcard_tests.rs` (6), outside the attributed function
bodies. These are source counts, not compiled-case totals. The destination
mapping is path-preserving from `ast/resolve[/...]` to
`sem/ast_conversion[/...]`, with the explicit `ast/document/merge[/...]` to
`sem/ast_conversion/merge[/...]` rule.

The DDL/ownership/view/engine ledger covers 11 old files. Its before/after
source counts match: 21 `#[test]`, 25 `#[rstest::rstest]`, 167 `#[case]`, two
`#[should_panic]`, and 156 assertion macros. Ownership test bodies are inlined
under `sem/ownership_conversion::tests`; CHECK and SERIAL syntax helpers remain
in AST while their semantic conversion and catalog effects are in sem. These
are source-preservation facts, not a final compiled-case count.

The API migrations intentionally change caller expressions
(`statement.analyze(...)` to `SemAst::from_ast(...)`, and the `Statement`
wildcard method to `WildcardExpansion::rewrite(...)`). Source comparisons
record these authorized migrations and their limits; compiled identity and
metadata preservation are established separately in the profile comparisons
above.

## Review-correction source preservation

Source-level preservation was checked against first PR head `22fca6e0` using
frozen source files and per-file comparisons:

- The 47 resolver and MERGE conversion files retain 334 attributed test
  functions, 1,166 `#[case]` attributes, 16 `#[values]` dimensions, 1,198
  whole-file assertion-macro calls, 10 TODOs, and 49 feature-gate attributes.
  Every file preserves its attribute multiset and whole-file assertion/TODO
  counts. Of the 1,198 assertion calls, 1,188 are inside test-attributed
  functions; the remaining 10 are outside those bodies.
- The 11 DDL and peripheral files retain 21 `#[test]` functions, 25
  `#[rstest::rstest]` functions, 167 case attributes, two `#[should_panic]`
  attributes, seven feature-gate attributes, 156 assertion-macro calls, and 22
  TODOs. Each file preserves its attribute multisets and assertion/TODO counts.
- The 48 caller files retain 122 test identities, with no unmatched tests or
  normalized body differences after the authorized API/import and formatting
  normalization. The comparison reported three unresolved receiver-type
  inferences for non-test helper calls (`owned_relation` twice and `semantic`
  once); those are not proof that caller API expressions stayed unchanged.
- Wildcard rewriting retains 16 main and four ROWS FROM test functions with
  attributes, cases, and normalized bodies preserved. Its compiled namespace
  changes from `wildcard_rewrite::` to
  `sem::wildcard_expansion::rewrite::`.

An independent source audit compared 373 changed test functions for literal
expectations and assertion order, without differences. It also compared mapped
conversion bodies for diagnostic literals and direct context-call order,
without a behavioral difference. These source comparisons supplement, but do
not replace, corrected final compiled-case reconciliation and execution.

## Existing consumers and regression inventory

- The server analyzes edited statements through `SemAst::from_ast` in
  `sql-fun-server/src/session/query_analysis.rs` and applies wildcard expansions
  through `WildcardExpansion::rewrite`. Its cache context remains caller-owned
  and is
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

## Review correction and validation status

The first PR revision (`22fca6e0`) reported passing execution checks, but those
results predate the current review corrections and do not validate the final
tree. It did not meet the implementation-ownership requirement. The maintainer
required eliminating the
generic source-typed conversion adapter and putting wildcard rewriting on
`WildcardExpansion`. These are explicit review requirements, not additional
feature scope. This correction changes the owning implementations and all
callers; no SQL behavior, semantic serialization, runtime dependency or
contributor workflow change is intended.

The current conversion index reflects these review corrections and the latest
source-map audit:

- Semantic conversion operations are inherent on the semantic result types;
  the `AstConversion<AST>` adapter and its parallel ownership layer are gone.
- Table-function output name rules are owned by `TableFunctionOutput`.
  `OperatorExpr::table_output_name(&self)` maps to the constant
  `TableFunctionOutput::nullif_output_name()` and takes no AST argument.
- The former `TypedColumns::resolve_function_output(&self, context)` wrapper
  is removed. Its direct replacement is
  `TableFunctionOutput::from_ast_typed_columns(columns: &[TypedColumn],
  context: &mut impl ParseContext)` in `sem/ast_conversion/query.rs`; callers
  pass the original syntax list.
- `SERIAL` syntax expansion remains the pure `ast/ddl/serial.rs` helper and is
  applied only to a temporary cloned column. Semantic type resolution,
  conflict validation, and sequence catalog effects remain in
  `sem/ddl_conversion/serial.rs`.
- Syntax-valued flags shared between decoding and semantic definitions live in
  neutral `syntax_options.rs`; shared `AnalysisError` lives in neutral
  `src/error.rs`. Neither introduces AST-to-sem dependencies.
- Wildcard rewriting is owned by `WildcardExpansion::rewrite` in
  `sem/wildcard_expansion/rewrite`; the crate-root rewrite consumer/export is
  removed.
- A misplaced SQLite feature gate on the moved `cursors` test modules hid 12
  default-profile cases. The module-level gate was removed. The independent
  cfg-attachment audit then found zero differences across 279 named modules,
  2,098 matched functions with inherited/direct gates, and 53 trait methods.
  This source audit complements the post-fix compiled-list reconciliations and
  standard-flow executions recorded above.

Before this correction, available worktree/build capacity was approximately
79 GiB. The additional build/test estimate is 20 GiB, reusing the existing
worktree's artifacts, above the repository's 20 GiB minimum baseline.

Post-correction default and all-features compiled reconciliations and standard
flows are complete as recorded above. Packaging, SemVer, and
installed-distribution checks also passed for
implementation commit `7a047313`, including all 3,064 extracted-binary SQL
regression cases.
Independent review found no
blocking ownership or correctness issue. It compared 373 changed test
functions for identity, attributes, literal expectations and assertion order
without differences, and checked conversion diagnostic literals and direct
context-call order. Source counts and compiled identities alone cannot
establish semantic behavior or ownership.

## Final semantic entry-point index

This index records old AST operations and their actual semantic destination
owners after review. Conversion entry points take the AST input explicitly;
there is no generic source adapter. Files containing the inherent semantic
implementations are mapped above. Multiple methods on one semantic type handle
its related input shapes; semantic representations and serialization remain
unchanged.

| Old AST receiver | Semantic destination owner | Old operation → destination operation |
| --- | --- | --- |
| `Access` | `ArraySubscript` | `resolve_subscript` → `from_ast_access` |
| `Access` | `IndirectionExpr` | `is_subscript` → `ast_is_subscript`, `resolve` → `from_ast_access`, `resolve_sequence` → `from_ast_sequence`, `subscript_prefix_length` → `ast_subscript_prefix_length` |
| `Alias` | `AliasSpec` | `resolve` → `from_ast` |
| `AlterCommand` | `AlterObjSubCommand` | `existing_column` → `check_ast_existing_column`, `missing_column` → `check_ast_missing_column`, `resolve` → `from_ast`, `resolve_add_column` → `from_ast_add_column`, `resolve_alter_column` → `from_ast_alter_column`, `resolve_and_report` → `from_ast_and_report`, `resolve_cluster` → `from_ast_cluster`, `resolve_column_attribute` → `from_ast_column_attribute`, `resolve_constraint` → `from_ast_constraint`, `resolve_default` → `from_ast_default`, `resolve_drop_column` → `from_ast_drop_column`, `resolve_owner` → `from_ast_owner`, `resolve_rename` → `from_ast_rename`, `resolve_rename_column` → `from_ast_rename_column`, `view_default_action` → `from_ast_view_default_action` |
| `AlterCommand` | `AlterTable` | `is_table_rename` → `is_ast_table_rename`, `table_rename_destination` → `rename_destination_from_ast` |
| `AlterCommand` | `ColumnAttributeAction` | `column_attribute_phase` → `phase_from_ast_command` |
| `AlterCommand` | `ColumnRename` | `column_rename_names` → `names_from_ast`, `is_column_rename` → `is_ast_command` |
| `AlterCommand` | `EngineFeatureValidation` | `check_engine_features` → `alter_command` |
| `AlterCommand` | `IndexAlteration` | `resolve_index` → `from_ast_command` |
| `AlterConstraintFlags` | `EngineFeatureValidation` | `check_engine_features` → `alter_constraint_flags` |
| `AlterDefinition` | `ColumnAttributeAction` | `statistics_request` → `statistics_from_ast_definition`, `validate_statistics` → `validate_ast_statistics` |
| `AlterDefinition` | `ColumnAttributeOption` | `column_options` → `from_ast_definition_list` |
| `AlterDefinition` | `EngineFeatureValidation` | `check_engine_features` → `alter_definition` |
| `AlterDefinition` | `IndexAlteration` | `index_statistics_target` → `statistics_from_ast_definition`, `resolve_index_statistics` → `from_ast_statistics_definition`, `resolve_index_storage` → `from_ast_storage_definition` |
| `AlterDefinition` | `IndexStorageParameter` | `index_storage_parameters` → `from_ast_definition_list` |
| `AlterDefinition` | `SemScalarExpr` | `view_default` → `from_ast_view_default_definition` |
| `AlterOperation` | `ColumnAttributeAction` | `column_attribute_action` → `from_ast_operation`, `column_attribute_phase` → `phase_from_ast_operation`, `drops_expression` → `ast_operation_drops_expression`, `drops_not_null` → `ast_operation_drops_not_null`, `sets_statistics` → `ast_operation_sets_statistics` |
| `AlterOperation` | `IndexAlteration` | `resolve_index` → `from_ast_operation` |
| `AlterOwner` | `AlterOwner` | `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply` |
| `AlterOwner` | `RequestedIdentity` | `target_identity` → `from_ast_owner_target` |
| `AlterRelation` | `AlterView` | `view_default_changes` → `from_ast_defaults` |
| `AlterRelation` | `EngineFeatureValidation` | `check_engine_features` → `alter_relation` |
| `AlterRelation` | `RelationAlteration` | `has_column_attributes` → `has_ast_column_attributes`, `has_column_rename` → `has_ast_column_rename`, `has_table_rename` → `has_ast_table_rename`, `resolve_and_apply` → `from_ast_and_apply`, `resolve_and_apply_at` → `from_ast_and_apply_at`, `resolve_and_apply_with_source` → `from_ast_and_apply_with_source`, `resolve_column_attributes` → `from_ast_column_attributes`, `resolve_column_rename` → `from_ast_column_rename`, `resolve_in` → `from_ast_inner`, `resolve_table_rename` → `from_ast_table_rename` |
| `AlterRelationKind` | `RelationAlteration` | `resolve` → `from_ast_kind`, `resolve_index` → `index_from_ast_commands`, `resolve_sequence` → `sequence_from_ast_commands`, `resolve_table` → `table_from_ast_commands`, `resolve_view` → `view_from_ast_commands` |
| `AlterSequence` | `AlterSequence` | `apply` → `from_ast_and_apply` |
| `Assignment` | `InsertTargetCollection` | `overlaps_mapped` → `ast_overlaps_mapped` |
| `Assignment` | `UpdateValue` | `resolve_update_value` → `from_ast_assignment`, `validate_update_value` → `validate_ast_update_value` |
| `BooleanExpr` | `SemScalarExpr` | `resolve` → `from_ast_boolean_expr` |
| `BooleanOperator` | `SemScalarExpr` | `resolve` → `from_ast_boolean_operator`, `resolve_not` → `from_ast_not` |
| `Call` | `DataSource` | `resolve_table` → `from_ast_table` |
| `Call` | `FuncCallExpr` | `resolve_supplied_call` → `from_ast_supplied_call` |
| `Call` | `FunctionModifiers` | `resolve_modifiers` → `from_ast_modifiers` |
| `Call` | `SemScalarExpr` | `resolve` → `from_ast_call`, `resolve_coalesce` → `from_ast_coalesce_call`, `resolve_min_max` → `from_ast_min_max_call`, `resolve_ordinary` → `from_ast_ordinary_call`, `uses_supplied_binding` → `ast_uses_supplied_binding` |
| `Call` | `TableFunctionOutput` | `table_output_name` → `ast_call_name` |
| `CallKind` | `SemScalarExpr` | `is_ordinary` → `ast_is_ordinary`, `resolve` → `from_ast_call_kind` |
| `Case` | `SemScalarExpr` | `resolve` → `from_ast_case` |
| `Cast` | `SemScalarExpr` | `resolve` → `from_ast_cast` |
| `Cast` | `TableFunctionOutput` | `table_output_name` → `ast_cast_name` |
| `Cast` | `ViewColumnLineage` | `view_cast_column` → `column_from_ast_cast` |
| `Collate` | `InsertInferenceModifiers` | `collect_insert_collations` → `collect_ast_collate_names` |
| `Collate` | `SemScalarExpr` | `resolve` → `from_ast_collate` |
| `Column` | `CheckColumnConstraint` | `has_inline_checks` → `has_ast_column_checks` |
| `Column` | `ColumnDefinition` | `resolve` → `from_ast`, `resolve_and_report` → `from_ast_and_report`, `resolve_constraints` → `resolve_ast_constraints`, `resolve_declaration` → `retain_ast_declaration`, `resolve_inline_checks` → `resolve_ast_inline_checks`, `resolve_serial` → `from_ast_serial`, `resolve_standalone` → `from_ast_standalone`, `resolve_type` → `type_from_ast` |
| `Column` | `ConstraintNames` | `reserve_constraint_names` → `reserve_ast_column` |
| `Column` | `EngineFeatureValidation` | `check_engine_features` → `column` |
| `Column` | `SerialSequences` | `serial_integer` → `integer_for_ast_column`, `validate_serial` → `validate_ast_column` |
| `ColumnConstraint` | `ColumnConstraint` | `resolve` → `from_ast`, `resolve_and_report` → `from_ast_and_report`, `resolve_standalone_check` → `from_ast_standalone_check` |
| `ColumnConstraint` | `Constraint` | `resolve_declared_foreign_key` → `from_ast_column_foreign_key`, `resolve_table` → `from_ast_column_table`, `resolve_table_primary_key` → `ast_table_primary_key`, `resolve_table_unique` → `ast_table_unique` |
| `ColumnConstraint` | `ConstraintNames` | `has_catalog_name` → `ast_constraint_has_catalog_name`, `supports_named_not_null` → `supports_named_not_null` |
| `ColumnConstraint` | `EngineFeatureValidation` | `check_enforcement` → `enforcement`, `check_engine_features` → `column_constraint`, `check_not_null` → `not_null` |
| `ColumnConstraint` | `SerialSequences` | `serial_conflict` → `conflict_from_ast_constraint` |
| `Comment` | `Comment` | `apply` → `from_ast_and_apply` |
| `CompositeField` | `CompositeFieldDefinition` | `resolve` → `from_ast` |
| `CompositeType` | `CreateCompositeType` | `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply` |
| `Conflict` | `InsertConflict` | `require_insert_action` → `require_ast_conflict_action`, `resolve_insert` → `from_ast_insert` |
| `ConflictAction` | `InsertConflict` | `require_insert_action` → `require_ast_action` |
| `CreateRule` | `CreateRule` | `check_columns` → `ast_check_columns`, `invalid` → `ast_invalid`, `resolve` → `from_ast`, `resolve_and_apply` → `apply_ast`, `resolve_definition` → `from_ast_definition`, `resolve_rule_predicate` → `from_ast_rule_predicate`, `validate_resolved` → `validate_ast_resolved`, `validate_results` → `validate_ast_results`, `validate_shape` → `validate_ast_shape` |
| `CreateTrigger` | `CreateTrigger` | `analysis_scope` → `ast_analysis_scope`, `finish_definition` → `ast_finish_definition`, `has_new` → `ast_has_new`, `has_old` → `ast_has_old`, `invalid` → `ast_invalid`, `relation` → `ast_relation`, `resolve` → `from_ast`, `resolve_and_apply` → `apply_ast`, `resolve_events` → `from_ast_events`, `resolve_identity` → `from_ast_identity`, `resolve_in` → `from_ast_in`, `resolve_predicate` → `from_ast_predicate`, `resolve_row_bindings` → `from_ast_row_bindings`, `resolve_target` → `from_ast_target`, `resolve_transitions` → `from_ast_transitions`, `validate` → `ast_validate`, `validate_identity` → `validate_ast_identity` |
| `CteBody` | `CteStatement` | `require_data_modifying_placement` → `ast_require_data_modifying_placement`, `resolve` → `from_ast_cte_body`, `validate_select_rule_ctes` → `validate_ast_select_rule_ctes` |
| `CteBody` | `EngineFeatureValidation` | `check_engine_features` → `cte_body` |
| `CurrentOf` | `DeleteCursor` | `resolve_delete` → `from_ast` |
| `CurrentOf` | `UpdateCursor` | `resolve_update` → `from_ast` |
| `DefaultPrivileges` | `EngineFeatureValidation` | `check_engine_features` → `default_privileges` |
| `Delete` | `DeleteStatement` | `resolve` → `from_ast`, `resolve_cte_body` → `from_ast_cte_body`, `resolve_destination` → `from_ast_destination`, `resolve_in` → `from_ast_in`, `resolve_predicate` → `from_ast_predicate`, `resolve_returning_scope` → `from_ast_returning_scope`, `resolve_sources` → `from_ast_sources`, `resolve_trigger_command` → `from_ast_trigger_command`, `validate_index_hint` → `validate_ast_index_hint` |
| `Delete` | `EngineFeatureValidation` | `check_engine_features` → `delete` |
| `DerivedTable` | `DataSource` | `resolve` → `from_ast_derived_table` |
| `Domain` | `CreateDomain` | `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply`, `resolve_in` → `from_ast_inner` |
| `Domain` | `EngineFeatureValidation` | `check_engine_features` → `domain` |
| `DropBehavior` | `DropDependencyBehavior` | `semantic` → `from_ast` |
| `DropRule` | `DropRule` | `resolve` → `from_ast`, `resolve_and_apply` → `apply_ast`, `resolve_definition` → `from_ast_definition`, `resolve_on` → `from_ast_on` |
| `DropTable` | `DropTable` | `apply` → `from_ast_and_apply` |
| `DropTrigger` | `DropTrigger` | `resolve` → `from_ast`, `resolve_and_apply` → `apply_ast`, `resolve_on` → `from_ast_on`, `resolve_postgres` → `from_ast_postgres`, `resolve_sqlite` → `from_ast_sqlite` |
| `EnumType` | `CreateEnum` | `apply` → `from_ast_and_apply` |
| `Expr` | `ColumnAttributeAction` | `statistics_integer` → `statistics_integer_from_ast_expression` |
| `Expr` | `ColumnAttributeOption` | `column_option_text` → `value_from_ast_expression` |
| `Expr` | `CreateSequence` | `owned_identifier` → `owned_identifier_from_ast_expression`, `owned_relation` → `owned_table_from_ast_expression`, `owned_relation_from_parts` → `owned_table_from_ast_parts` |
| `Expr` | `DataSource` | `resolve_expression_table` → `from_ast_expression_table`, `resolve_table_call` → `from_ast_table_call` |
| `Expr` | `DataSourceRowsFromFunc` | `resolve_rows` → `from_ast`, `resolve_rows_call` → `from_ast_call` |
| `Expr` | `DeleteCursor` | `resolve_delete_current_of` → `from_ast_delete_current_of` |
| `Expr` | `IndexAlteration` | `index_statistics_target` → `statistics_from_ast_expr` |
| `Expr` | `IndexStorageParameter` | `index_option_value` → `value_from_ast_expr` |
| `Expr` | `InsertInferenceModifiers` | `collect_insert_collations` → `collect_ast_collations`, `collect_parenthesized_insert_collations` → `collect_ast_parenthesized_collations` |
| `Expr` | `InsertValue` | `resolve_insert_value` → `from_ast` |
| `Expr` | `QueryOrder` | `ordinal` → `ast_ordinal`, `sqlite_comparison_sql` → `ast_sqlite_comparison_sql`, `unqualified_column` → `ast_unqualified_column` |
| `Expr` | `SemScalarExpr` | `invalid_star_projection` → `ast_invalid_star_projection`, `is_named_argument` → `ast_is_named_argument`, `is_rule_boolean_literal` → `ast_is_rule_boolean`, `negated_literal` → `negated_ast_literal`, `projection_base` → `ast_projection_base`, `resolve` → `from_ast`, `resolve_array` → `from_ast_array`, `resolve_array_target` → `from_ast_array_target`, `resolve_clause` → `from_ast_clause`, `resolve_from_expression` → `from_ast_source_expression`, `resolve_grouping` → `from_ast_grouping`, `resolve_in` → `from_ast_in`, `resolve_in_body` → `from_ast_in_body`, `resolve_insert_inference` → `from_ast_insert_inference`, `resolve_list` → `from_ast_list`, `resolve_merge_action` → `from_ast_merge_action`, `resolve_merge_predicate` → `from_ast_merge_predicate`, `resolve_optional` → `from_ast_optional`, `resolve_parenthesized` → `from_ast_parenthesized`, `resolve_parenthesized_insert_inference` → `from_ast_parenthesized_insert_inference`, `resolve_row` → `from_ast_row`, `resolve_scalar_in` → `from_ast_scalar_in`, `resolve_scalar_list` → `from_ast_scalar_list`, `resolved_view_default` → `from_ast_view_default_expression`, `untyped_trigger_predicate` → `ast_is_untyped_trigger_predicate` |
| `Expr` | `TableFunctionOutput` | `table_output_name` → `ast_expression_name` |
| `Expr` | `UpdateCursor` | `resolve_current_of` → `from_ast_current_of` |
| `Expr` | `UpdateValue` | `resolve_update_row` → `from_ast_row`, `resolve_update_row_values` → `from_ast_update_row_values`, `resolve_update_value` → `from_ast_expr` |
| `Expr` | `ViewColumnLineage` | `view_cast_column` → `column_from_ast_expression` |
| `Extension` | `CreateExtension` | `apply` → `from_ast_and_apply` |
| `ForeignReference` | `ColumnConstraint` | `resolve` → `from_ast_foreign_reference` |
| `ForeignReference` | `Constraint` | `referenced_columns` → `referenced_columns_from_ast`, `report_ignored_matches` → `report_ast_ignored_matches`, `resolve_table` → `from_ast_foreign_reference`, `resolve_table_in` → `from_ast_foreign_reference_in` |
| `ForeignTable` | `EngineFeatureValidation` | `check_engine_features` → `foreign_table` |
| `Frame` | `QueryFrame` | `resolve` → `from_ast` |
| `FrameBound` | `QueryFrameBound` | `resolve` → `from_ast` |
| `FrameExclusion` | `QueryFrame` | `semantic_name` → `ast_exclusion_name` |
| `FrameMode` | `QueryFrame` | `semantic_name` → `ast_mode_name` |
| `FromGroup` | `DataSource` | `resolve` → `from_ast_from_group` |
| `FromItem` | `DataSource` | `resolve` → `from_ast`, `resolve_body` → `from_ast_body`, `resolve_in_scope` → `from_ast_in_scope`, `resolve_list` → `from_ast_list`, `resolve_list_in` → `from_ast_list_in`, `resolve_list_in_scope` → `from_ast_list_in_scope`, `resolve_source` → `from_ast_source` |
| `FromItem` | `ViewColumnLineage` | `automatic_view_source` → `source_from_ast_from_item` |
| `Function` | `CreateFunction` | `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply`, `resolve_in` → `from_ast_inner` |
| `FunctionParameter` | `FunctionParam` | `resolve` → `from_ast`, `resolve_and_report` → `from_ast_and_report` |
| `Grouping` | `QueryGrouping` | `resolve` → `from_ast`, `resolve_expression` → `from_ast_expression`, `resolve_list` → `from_ast_list` |
| `Index` | `CreateIndex` | `initialize_index_storage` → `retain_ast_storage`, `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply`, `resolve_elements` → `elements_from_ast`, `resolve_elements_in` → `elements_from_ast_in`, `resolve_in` → `from_ast_inner` |
| `IndexExpression` | `InsertInferenceModifiers` | `resolve_insert_modifiers` → `from_ast_insert_modifiers` |
| `Indirection` | `SemScalarExpr` | `projection_base` → `ast_indirection_projection_base`, `resolve` → `from_ast_indirection` |
| `Insert` | `EngineFeatureValidation` | `check_engine_features` → `insert` |
| `Insert` | `InsertStatement` | `resolve` → `from_ast`, `resolve_cte_body` → `from_ast_cte_body`, `resolve_definition` → `from_ast_definition`, `resolve_in` → `from_ast_in`, `resolve_trigger_command` → `from_ast_trigger_command`, `validate_expanded_source` → `validate_ast_expanded_source` |
| `Into` | `SelectDestination` | `resolve_destination` → `from_ast_destination` |
| `Join` | `DataSource` | `resolve` → `from_ast_join` |
| `Join` | `JoinUsingClause` | `using_clause` → `ast_using_clause` |
| `JoinKind` | `JoinKind` | `resolve` → `from_ast` |
| `Literal` | `ColumnAttributeAction` | `statistics_integer` → `statistics_integer_from_ast_literal` |
| `Literal` | `ColumnAttributeOption` | `column_option_text` → `value_from_ast_literal` |
| `Literal` | `CreateSequence` | `owned_identifier` → `owned_identifier_from_ast_literal`, `owned_relation` → `owned_table_from_ast_literal` |
| `Literal` | `IndexAlteration` | `index_statistics_target` → `statistics_from_ast_literal` |
| `Literal` | `IndexStorageParameter` | `index_option_value` → `value_from_ast_literal` |
| `Literal` | `SemScalarExpr` | `is_rule_boolean_literal` → `ast_literal_is_rule_boolean`, `negate` → `negate_ast_literal`, `resolve` → `from_ast_literal`, `untyped_trigger_predicate` → `ast_literal_is_untyped_trigger_predicate` |
| `Lock` | `QueryLock` | `resolve_list` → `from_ast_list` |
| `LockStrength` | `QueryLock` | `semantic_name` → `ast_strength_name` |
| `LockWait` | `QueryLock` | `semantic_name` → `ast_wait_name` |
| `Merge` | `EngineFeatureValidation` | `check_engine_features` → `merge` |
| `Merge` | `MergeStatement` | `require_version` → `ast_require_version`, `resolve` → `from_ast`, `resolve_clauses` → `from_ast_clauses`, `resolve_cte_body` → `from_ast_cte_body`, `resolve_in` → `from_ast_in` |
| `MergeMatchKind` | `MergeMatchKind` | `index` → `ast_index`, `permits` → `ast_permits`, `resolve` → `from_ast_merge_match_kind` |
| `MergeWhen` | `MergeAction` | `resolve_action` → `from_ast_action`, `resolve_insert_action` → `from_ast_insert_action`, `resolve_update_action` → `from_ast_update_action` |
| `Name` | `AliasName` | `star_qualifier` → `from_ast_star_qualifier` |
| `Name` | `CreateSequence` | `owned_relation` → `owned_table_from_ast_name` |
| `Name` | `FullName` | `full_name` → `from_ast_name`, `optional_full_name` → `from_optional_ast_name`, `star_relation` → `from_ast_star_relation` |
| `Name` | `IndexStorageParameter` | `index_option_word` → `word_from_ast_name` |
| `Name` | `OperatorName` | `operator_name` → `from_ast_name` |
| `Name` | `QueryOrder` | `unqualified` → `ast_unqualified_name` |
| `Name` | `ResultColumnCollection` | `require_returning_star` → `require_ast_returning_star` |
| `Name` | `SemScalarExpr` | `append_row_projection` → `append_ast_row_projection`, `resolve_column` → `from_ast_column` |
| `Name` | `TableFunctionOutput` | `table_output_name` → `ast_relation_name` |
| `Name` | `ViewColumnLineage` | `view_source_column` → `column_from_ast_name` |
| `NamedArgument` | `SemScalarExpr` | `resolve` → `from_ast_named_argument` |
| `NullTest` | `SemScalarExpr` | `resolve` → `from_ast_null_test` |
| `ObjectTarget` | `RequestedIdentity` | `ownership_identity` → `from_ast_object_target` |
| `OperatorExpr` | `SemScalarExpr` | `distinctness` → `ast_expression_distinctness`, `resolve` → `from_ast_operator_expr`, `resolve_operator` → `from_ast_operator_operands`, `resolve_sqlite_pattern` → `from_ast_sqlite_pattern` |
| `OperatorExpr` | `TableFunctionOutput` | `table_output_name(&self)` → `nullif_output_name()` (constant helper; no AST argument) |
| `OperatorKind` | `ArithExpr` | `distinctness` → `ast_operator_distinctness`, `has_named_operator` → `ast_has_named_operator`, `is_not_like` → `ast_is_not_like`, `is_null_if` → `ast_is_null_if`, `is_pattern` → `ast_is_pattern`, `membership_negated` → `ast_membership_negated`, `resolve` → `from_ast_operator_kind`, `resolve_between` → `from_ast_between`, `resolve_symmetric_between` → `from_ast_symmetric_between`, `without_catalog_operator` → `ast_without_catalog_operator` |
| `OperatorKind` | `ResolvedOperator` | `resolve_operator_name` → `from_ast_operator_name` |
| `Order` | `QueryOrder` | `checked_name` → `ast_checked_name`, `resolve` → `from_ast`, `resolve_for_query` → `from_ast_for_query`, `resolve_list` → `from_ast_list`, `resolve_using_operator` → `from_ast_using_operator` |
| `OverridingKind` | `InsertClauseSemantics` | `insert_mode` → `ast_insert_mode`, `merge_override` → `ast_merge_override`, `overrides_identity` → `ast_overrides_identity` |
| `ParameterMode` | `FunctionParam` | `resolve_parameter` → `from_ast_mode` |
| `PostgresAlterCommand` | `AlterObjSubCommand` | `resolve` → `from_ast_postgres`, `resolve_column_attribute` → `from_ast_postgres_column_attribute`, `resolve_owner` → `from_ast_postgres_owner`, `resolve_single_column_attribute` → `from_ast_single_column_attribute`, `view_column_default` → `from_ast_postgres_view_column_default`, `view_default_action` → `from_ast_postgres_view_default_action` |
| `PostgresAlterCommand` | `IndexAlteration` | `resolve_index` → `from_ast_postgres_command` |
| `Query` | `EngineFeatureValidation` | `check_engine_features` → `query` |
| `Query` | `InsertSource` | `resolve_insert_source` → `from_ast` |
| `Query` | `SelectStatement` | `resolve` → `from_ast`, `resolve_cte_body` → `from_ast_cte_body`, `resolve_in` → `from_ast_in`, `resolve_in_body` → `from_ast_in_body`, `resolve_query_body` → `analyze_ast_query`, `resolve_test_in` → `from_ast_test_in`, `validate_select_rule_ctes` → `validate_ast_select_rule_ctes` |
| `Query` | `ViewColumnLineage` | `automatic_view_source` → `source_from_ast_query`, `refine_view_cast_lineage` → `refine_ast_casts`, `view_lineage` → `from_ast_query` |
| `QueryBody` | `QueryOrder` | `match_sqlite_order` → `match_ast_compound_order`, `match_sqlite_values_order` → `match_ast_values_order`, `order_binding` → `ast_order_binding` |
| `QueryBody` | `SelectStatement` | `is_compound` → `ast_is_compound`, `resolve` → `from_ast_body`, `resolve_values` → `from_ast_values`, `set_all` → `ast_set_all`, `window_names` → `ast_window_names` |
| `QueryBody` | `ViewColumnLineage` | `automatic_view_source` → `source_from_ast_query_body` |
| `Relation` | `DataSource` | `resolve` → `from_ast_relation`, `validate_trigger_command` → `validate_ast_trigger_command` |
| `ReturningOption` | `EngineFeatureValidation` | `check_features` → `returning_options` |
| `Role` | `OwnerSpecification` | `resolve_named_owner` → `from_ast_named_role`, `resolve_owner` → `from_ast` |
| `RuleEvent` | `RuleEvent` | `semantic` → `ast_semantic` |
| `Schema` | `CreateSchema` | `resolve_and_apply` → `from_ast_and_apply`, `resolve_group` → `from_ast_group` |
| `Select` | `QueryOrder` | `match_sqlite_order` → `match_ast_select_order` |
| `Select` | `SelectStatement` | `build_result` → `ast_build_result`, `resolve` → `from_ast_select`, `resolve_clauses` → `from_ast_clauses`, `resolve_distinct` → `from_ast_distinct`, `resolve_from` → `from_ast_from`, `resolve_grouping` → `from_ast_grouping`, `resolve_having` → `from_ast_having`, `resolve_targets` → `from_ast_targets`, `resolve_where` → `from_ast_where`, `resolve_windows` → `from_ast_windows`, `scope` → `ast_scope`, `validate_rule_destination` → `validate_ast_rule_destination` |
| `Sequence` | `CreateSequence` | `apply` → `from_ast_and_apply` |
| `SetOperation` | `SelectStatement` | `resolve` → `from_ast_set_operation` |
| `SetQuery` | `QueryOrder` | `match_sqlite_order` → `match_ast_set_order` |
| `SetQuery` | `SelectStatement` | `resolve` → `from_ast_set` |
| `SqlValue` | `SemScalarExpr` | `resolve` → `from_ast_sql_value` |
| `SqlValue` | `TableFunctionOutput` | `table_output_name` → `ast_sql_value_name` |
| `StatementNode` | `EngineFeatureValidation` | `check_engine_features` → `statement` |
| `StatementNode` | `SemAst` | `resolve` → `from_ast_node`, `resolve_and_apply` → `apply_ast_node`, `resolve_rule_action` → `from_ast_rule_action`, `resolve_trigger_command` → `from_ast_trigger_command`, `rule_query` → `ast_rule_query`, `validate_select_rule_ctes` → `validate_ast_select_rule_ctes` |
| `Subquery` | `SemScalarExpr` | `resolve` → `from_ast_subquery`, `resolve_update_row` → `from_ast_subquery_row` |
| `SubqueryKind` | `SemScalarExpr` | `is_membership` → `ast_is_membership`, `pending_rule_semantics` → `ast_pending_rule_semantics`, `resolve` → `from_ast_subquery_kind` |
| `SyntaxOption` | `ColumnAttributeOption` | `column_attribute_option` → `from_ast` |
| `SyntaxOption` | `CreateSequence` | `owned_relation` → `owned_table_from_ast_option` |
| `SyntaxOption` | `IndexStorageParameter` | `index_storage_parameters` → `from_ast_list` |
| `SyntaxValue` | `ColumnAttributeOption` | `column_option_text` → `value_from_ast` |
| `SyntaxValue` | `IndexStorageParameter` | `index_option_value` → `value_from_ast_syntaxvalue` |
| `Table` | `CreateTable` | `constraint_names` → `constraint_names_from_ast`, `resolve` → `from_ast`, `resolve_and_apply` → `from_ast_and_apply`, `resolve_and_apply_at` → `from_ast_and_apply_at`, `resolve_checks` → `resolve_ast_checks`, `resolve_foreign_keys` → `resolve_ast_foreign_keys`, `resolve_in` → `from_ast_inner`, `resolve_inline_checks` → `resolve_ast_inline_checks`, `resolve_with_names` → `from_ast_with_names` |
| `Table` | `EngineFeatureValidation` | `check_engine_features` → `table` |
| `TableAs` | `CreateTableAs` | `resolve_and_apply` → `from_ast_and_apply`, `resolve_and_apply_at` → `from_ast_and_apply_at` |
| `TableConstraint` | `CheckColumnConstraint` | `resolve_legacy_inline_check` → `from_ast_legacy_inline` |
| `TableConstraint` | `CheckConstraint` | `analyze_legacy_check` → `from_legacy_syntax`, `resolve_check` → `from_ast_table_constraint`, `resolve_check_expression` → `from_ast_expression` |
| `TableConstraint` | `Constraint` | `column_names` → `columns_from_ast`, `delete_column_names` → `delete_columns_from_ast`, `resolve` → `from_ast_table`, `resolve_declared_foreign_key` → `from_ast_declared_foreign_key`, `resolve_declared_key` → `from_ast_declared_key` |
| `TableConstraint` | `EngineFeatureValidation` | `check_engine_features` → `table_constraint`, `check_temporal_features` → `temporal_constraint` |
| `TableFunctions` | `DataSource` | `resolve` → `from_ast_table_functions`, `resolve_rows_from` → `from_ast_rows_from` |
| `TableSample` | `DataSource` | `resolve` → `from_ast_table_sample` |
| `Target` | `ResultColumnCollection` | `append_source_star` → `ast_append_source_star`, `append_to_results` → `ast_append_to_results` |
| `TypedColumns` | `TableFunctionOutput` | `resolve_function_output` → `from_ast_typed_columns(&[TypedColumn], context)` (source-list wrapper removed; caller passes the AST list directly) |
| `TriggerBody` | `TriggerBody` | `resolve` → `from_ast` |
| `TriggerConstraint` | `TriggerConstraint` | `resolve` → `from_ast` |
| `TriggerEvent` | `TriggerEvent` | `has_columns` → `ast_has_columns`, `has_new` → `ast_has_new`, `has_old` → `ast_has_old`, `is_truncate` → `ast_is_truncate`, `resolve` → `from_ast` |
| `TriggerFunction` | `TriggerFunction` | `resolve` → `from_ast` |
| `TriggerGranularity` | `TriggerGranularity` | `semantic` → `ast_semantic` |
| `TriggerTiming` | `TriggerTiming` | `semantic` → `ast_semantic` |
| `TruthTest` | `SemScalarExpr` | `resolve` → `from_ast_truth_test` |
| `TypeName` | `IndexStorageParameter` | `index_option_word` → `word_from_ast_typename` |
| `TypeName` | `OwnershipIdentity` | `owner_argument_type` → `argument_type_from_ast` |
| `TypeName` | `SerialSequences` | `serial_integer` → `integer_for_ast_type` |
| `TypeName` | `TypeReference` | `reference` → `from_ast` |
| `TypeName` | `WithTypmod<TypeReference>` | `declaration_type` → `from_ast_declaration`, `modifier_arguments` → `ast_modifier_arguments` |
| `Update` | `EngineFeatureValidation` | `check_engine_features` → `update` |
| `Update` | `UpdateStatement` | `resolve` → `from_ast`, `resolve_cte_body` → `from_ast_cte_body`, `resolve_definition` → `from_ast_definition`, `resolve_destination` → `from_ast_destination`, `resolve_in` → `from_ast_in`, `resolve_predicate` → `from_ast_predicate`, `resolve_returning_scope` → `from_ast_returning_scope`, `resolve_sources` → `from_ast_sources`, `resolve_trigger_command` → `from_ast_trigger_command`, `validate_index_hint` → `validate_ast_index_hint`, `validate_view_trigger` → `validate_ast_view_trigger` |
| `VariableSet` | `VariableSet` | `resolve_and_apply` → `from_ast_and_apply` |
| `View` | `CreateView` | `resolve_and_apply` → `from_ast_and_apply`, `semantic_options` → `from_ast_options` |
| `ViewCheckOption` | `ViewCheckOption` | `semantic` → `from_ast` |
| `Window` | `QueryWindow` | `names` → `ast_names`, `resolve` → `from_ast`, `resolve_definitions` → `from_ast_definitions`, `resolve_over` → `from_ast_over` |
| `With` | `EngineFeatureValidation` | `check_engine_features` → `with_clause` |
| `With` | `WithClause` | `local_definitions` → `ast_local_definitions`, `resolve` → `from_ast`, `resolve_with` → `from_ast_top_level` |
| `XmlTable` | `DataSource` | `resolve` → `from_ast_xml_table` |

`TableFunctionOutput::from_ast_typed_columns` is defined in
`sem/ast_conversion/query.rs`. It consumes the original syntax list directly;
no borrowed `TypedColumns` adapter remains. The NULLIF output-name helper is a
constant and accepts no AST node because its caller already selected that
semantic operation.
