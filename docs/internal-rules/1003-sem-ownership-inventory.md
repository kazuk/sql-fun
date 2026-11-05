# #1003 semantic ownership baseline and final source map

This reviewer-facing ledger records the old AST source set at base revision
`bf81afb3e31fc03b4bc10e792d66b79b4601f535` and its disposition in source
snapshot `accb7d65` and round-three consolidation (2026-11-05). It uses the
frozen source tree for baseline facts and current files for destinations. The
106-row table
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

- Production AST code has no semantic dependency. The only AST-to-sem import
  is in the `#[cfg(test)]` synchronization suite at
  `ast/public/wildcard.rs`, where three syntax-location tests use `SemAst` and
  `WildcardExpansion` as an analysis oracle. AST production code keeps dialect
  decoding into the common AST, inspection, mutation, traversal, structural
  validation, and deparse.
- Conversion implementations are inherent operations on their semantic result
  types: scalar expressions on `SemScalarExpr`, queries on `SelectStatement`,
  DML on their semantic statement types, and DDL on their semantic definitions.
  Statement dispatch and application belong to `SemAst`; version validation
  and multi-kind ALTER orchestration use concrete semantic processing
  components. Each receives common AST explicitly, alongside the existing
  context. The source-typed generic adapter from the first PR revision is
  removed; it is not retained as a renamed service or compatibility facade.
  The old-operation index records semantic owners; the physical destination
  tables below give the current defining-module paths. No catch-all AST or DDL
  conversion module remains.
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
- `AnalysisError` remains defined in `src/error.rs`, including its syntax-level
  dialect compatibility check; `sem.rs` re-exports it. `ProblemLikeError` is
  defined in `sem.rs`. Its public declaration keeps the existing
  `ValidOperatorName::try_from` return type nameable after moving it out of the
  private `sem/error` module; this adds the public path
  `sem::ProblemLikeError` without changing that method signature or the error
  variants. Type-specific error helpers live beside their owners in
  `sem/create_comp_type.rs` and `sem/pg_type/type_ref.rs`. AST syntax
  conversions import the root error type directly.
- Syntax-valued option types are AST-owned in `ast/options.rs`, which contains
  their parser conversions, documentation, and inline tests. Option deparsing
  is in `ast/options/deparse.rs`. The former `syntax_options.rs` module is gone.
  The ten type paths formerly re-exported under `sem` are removed; consumers
  import `ast` directly. The AST type definitions and serialization stay the
  same, with no semantic aliases retained.
- SERIAL keeps its pure AST helper in `ast/ddl.rs`: it clones a column
  node and replaces the SERIAL spelling with its integer type.
  `sem/create_table/serial.rs` applies it to a temporary node, resolves that
  temporary into a semantic column, validates conflicts, and stages
  `CreateSequence` catalog effects. The expanded syntax is not written back into
  the user's AST or retained as semantic state.
- `ast/validation.rs` and `ast/document_validation.rs` retain structural syntax
  validation and use the crate stack operation directly.
  `ast/default_privileges.rs` and `ast/foreign_table.rs` retain syntax only;
  their analysis branches remain the documented TODOs #955 and #956 in sem
  dispatch. No SQL support was added by this ownership move.

## Physical implementation and test-placement map

The physical maps were established at correction baseline
`0d4fe0edfdf04eaed0f9ecbaab43b8e113386df5` and reaudited after round-three
consolidation at source snapshot `accb7d65`. They supersede the earlier
`sem/ast_conversion`, `sem/ddl_conversion`, `sem/ownership_conversion`, and
`sem/view_conversion` paths in the first-pass source ledger. The semantic
owner index below identifies each old operation; these tables give current
defining files after recursive module absorption.

### Expression, query, and statement conversion implementations

318 implementation blocks are placed in 33 current
owner-specific files. The owner names and methods are indexed in the semantic
entry-point table below.

| Physical destination | Impl blocks | Owner types |
| --- | ---: | --- |
| `names.rs` | 1 | AliasName |
| `names/full.rs` | 3 | FullName |
| `names/ope.rs` | 1 | OperatorName |
| `sem/ast.rs` | 4 | SemAst |
| `sem/conflict_info.rs` | 1 | CatalogNameLookup |
| `sem/create_rule/ast.rs` | 13 | CreateRule, PredicateValidator, RuleEvent |
| `sem/create_trigger/ast.rs` | 31 | CreateTrigger, TriggerBody, TriggerConstraint, TriggerEvent, TriggerFunction, TriggerGranularity, TriggerTiming, WhenValidator |
| `sem/cte/ast.rs` | 6 | CteStatement, WithClause |
| `sem/data_source/alias.rs` | 1 | AliasSpec |
| `sem/data_source/ast.rs` | 19 | DataSource |
| `sem/data_source/function_columns.rs` | 7 | TableFunctionOutput |
| `sem/data_source/join_expr.rs` | 1 | JoinUsingClause |
| `sem/data_source/join_kind.rs` | 1 | JoinKind |
| `sem/data_source/rows_fun.rs` | 2 | DataSourceRowsFromFunc |
| `sem/delete_statement/ast.rs` | 9 | DeleteStatement |
| `sem/delete_statement/cursor.rs` | 2 | DeleteCursor |
| `sem.rs` | 1 | DropDependencyBehavior |
| `sem/drop_rule_ast.rs` | 4 | DropRule |
| `sem/drop_trigger/ast.rs` | 5 | DropTrigger |
| `sem/insert_statement/ast.rs` | 30 | InsertClauseSemantics, InsertConflict, InsertInferenceModifiers, InsertSource, InsertStatement, InsertTarget, InsertTargetBinding, InsertTargetCollection, InsertTargetMode, InsertValue, InsertValueCollection |
| `sem/merge_statement/ast.rs` | 12 | MergeAction, MergeMatchKind, MergeStatement |
| `sem/pg_type/type_ref.rs` | 2 | TypeReference |
| `sem/query/ast.rs` | 33 | ClauseBinding, FunctionModifiers, QueryFrame, QueryFrameBound, QueryGrouping, QueryLock, QueryOrder, QueryWindow, ResolvedOperator, SelectDestination |
| `sem/scalar_expr/arith_expr/ast.rs` | 10 | ArithExpr |
| `sem/scalar_expr/ast.rs` | 59 | SemScalarExpr |
| `sem/scalar_expr/func_call.rs` | 1 | FuncCallExpr |
| `sem/scalar_expr/indirect/array_subscripts.rs` | 2 | ArraySubscript |
| `sem/scalar_expr/indirect.rs` | 5 | IndirectionExpr |
| `sem/select_statement/ast.rs` | 26 | SelectStatement |
| `sem/select_statement/result_column/ast.rs` | 5 | ResultColumnCollection |
| `sem/update_statement/ast.rs` | 17 | UpdateAssignmentCollection, UpdateStatement, UpdateValue |
| `sem/update_statement/cursor.rs` | 2 | UpdateCursor |
| `sem/with_typmod.rs` | 2 | WithTypmod |

### DDL and semantic option implementations

120 struct/implementation items map to 41 current
physical destinations. The rows include semantic definitions, conversions,
and shared DDL helpers.

| Physical destination | Items | Owner types |
| --- | ---: | --- |
| `sem/alter_column/ddl.rs` | 8 | ColumnAttributeAction, ColumnAttributeOption |
| `sem/alter_index/ddl.rs` | 10 | IndexAlteration, IndexStorageParameter |
| `sem/alter_owner/ddl.rs` | 4 | AlterOwner, RequestedIdentity |
| `sem/alter_seq.rs` | 1 | AlterSequence |
| `sem/alter_table/ddl.rs` | 7 | AlterObjSubCommand, AlterTable, AlterView |
| `sem/column_constraints/attr_constraint.rs` | 3 | AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint, AttrImmediateColumnConstraint |
| `sem/column_constraints/check_constraint.rs` | 2 | CheckColumnConstraint |
| `sem/column_constraints/collate_constraint.rs` | 1 | CollateColumnConstraint |
| `sem/column_constraints/ddl.rs` | 2 | ColumnConstraint |
| `sem/column_constraints/default_constraint.rs` | 1 | DefaultColumnConstraint |
| `sem/column_constraints/exclusion_constraint.rs` | 1 | ExclusionColumnConstraint |
| `sem/column_constraints/generated_constraint.rs` | 1 | GeneratedColumnConstraint |
| `sem/column_constraints/identity_constraint.rs` | 1 | IdentityColumnConstraint |
| `sem/column_constraints/nullability_constraint.rs` | 1 | ColumnNullabilityConstraint |
| `sem/column_constraints/primary_key_constraint.rs` | 1 | PrimaryKeyColumnConstraint |
| `sem/column_constraints/unique_constraint.rs` | 1 | UniqueColumnConstraint |
| `sem/column_rename.rs` | 1 | ColumnRename |
| `sem.rs` | 2 | Comment, CreateExtension |
| `sem/constraint/check.rs` | 1 | CheckConstraint |
| `sem/constraint/ddl.rs` | 3 | Constraint |
| `sem/constraint.rs` | 2 | ConstraintNames |
| `sem/create_comp_type/ddl.rs` | 3 | CompositeFieldDefinition, CreateCompositeType |
| `sem/create_domain/ddl.rs` | 3 | CreateDomain |
| `sem/create_enum_type.rs` | 1 | CreateEnum |
| `sem/create_function/ddl.rs` | 3 | CreateFunction |
| `sem/create_index/ddl.rs` | 2 | CreateIndex |
| `sem/create_schema.rs` | 1 | CreateSchema |
| `sem/create_seq/ddl.rs` | 2 | CreateSequence |
| `sem/create_table/column_def/ddl.rs` | 4 | ColumnDefinition |
| `sem/create_table/ddl.rs` | 4 | CreateTable, DropTable |
| `sem/create_table/serial.rs` | 6 | SerialSequences |
| `sem/create_table_as.rs` | 1 | CreateTableAs |
| `sem/create_view/ddl.rs` | 2 | CreateView |
| `sem/engine_features.rs` | 9 | EngineFeatureValidation |
| `sem/ownership/ddl.rs` | 4 | OwnerSpecification, OwnershipIdentity |
| `sem/relation_alteration.rs` | 8 | RelationAlteration |
| `sem/scalar_expr.rs` | 2 | SemScalarExpr |
| `sem/sem_ast_analysis.rs` | 7 | SemAst |
| `sem/type_system/func_params.rs` | 1 | FunctionParam |
| `sem/variable_set.rs` | 1 | VariableSet |
| `sem/view_definition/ddl.rs` | 2 | ViewCheckOption, ViewColumnLineage |

### Small-module absorption review

The frozen round-three review started from source snapshot `accb7d65` and
classified 105 changed-module candidates. It absorbed all 51 introduced small
fragments, eight extraction remainders, and five recursive parent modules: 64
absorptions covering 2,399 lines at merge time. Of 11 fragmented
existing parents, five were recursively absorbed and six remain as owner files
after their child declarations moved. The six retained owners are all over 100
lines: `sem/alter_seq.rs` (129),
`sem/column_constraints/exclusion_constraint.rs` (101),
`sem/create_schema.rs` (123), `sem/delete_statement/cursor.rs` (141),
`sem/update_statement/cursor.rs` (138), and `sem/variable_set.rs` (114). No
eligible in-scope module at or below 100 lines remains. The review excluded 33
files changed only for imports, callers, or formatting, plus two pure
relocations of existing integration-test modules. The frozen map records 18
identifier adjustments, mostly to reuse existing imports or resolve name
collisions. Its visibility adjustment moves
`ProblemLikeError` into public `sem.rs` with its `pub` declaration intact, so
the existing public method signature remains nameable. It introduced no
namespace-wrapper facades or new abstractions.

The table records every absorbed source module and its final owner file. Source
paths here document the pre-absorption module layout; destination paths name
current files. Line counts are the source-module sizes at absorption.

| Module before absorption | Lines | Current owner file |
| --- | ---: | --- |
| `sem/scalar_expr/func_call/implementation/ast.rs` | 24 | `sem/scalar_expr/func_call.rs` |
| `sem/scalar_expr/indirect/array_subscripts/ast.rs` | 40 | `sem/scalar_expr/indirect/array_subscripts.rs` |
| `sem/column_constraints/attr_constraint/ddl.rs` | 41 | `sem/column_constraints/attr_constraint.rs` |
| `sem/column_constraints/check_constraint/ddl.rs` | 45 | `sem/column_constraints/check_constraint.rs` |
| `sem/column_constraints/collate_constraint/ddl.rs` | 21 | `sem/column_constraints/collate_constraint.rs` |
| `sem/column_constraints/default_constraint/ddl.rs` | 23 | `sem/column_constraints/default_constraint.rs` |
| `sem/column_constraints/exclusion_constraint/ddl.rs` | 17 | `sem/column_constraints/exclusion_constraint.rs` |
| `sem/column_constraints/generated_constraint/ddl.rs` | 25 | `sem/column_constraints/generated_constraint.rs` |
| `sem/column_constraints/identity_constraint/ddl.rs` | 19 | `sem/column_constraints/identity_constraint.rs` |
| `sem/column_constraints/nullability_constraint/ddl.rs` | 21 | `sem/column_constraints/nullability_constraint.rs` |
| `sem/column_constraints/primary_key_constraint/ddl.rs` | 20 | `sem/column_constraints/primary_key_constraint.rs` |
| `sem/column_constraints/unique_constraint/ddl.rs` | 19 | `sem/column_constraints/unique_constraint.rs` |
| `sem/constraint/check/ddl.rs` | 94 | `sem/constraint/check.rs` |
| `sem/constraint/names/ddl.rs` | 53 | `sem/constraint.rs` |
| `sem/data_source/alias/ast.rs` | 7 | `sem/data_source/alias.rs` |
| `sem/data_source/function_columns/ast.rs` | 94 | `sem/data_source/function_columns.rs` |
| `sem/data_source/join_expr/ast.rs` | 21 | `sem/data_source/join_expr.rs` |
| `sem/data_source/join_kind/ast.rs` | 12 | `sem/data_source/join_kind.rs` |
| `sem/data_source/rows_fun/ast.rs` | 50 | `sem/data_source/rows_fun.rs` |
| `sem/delete_statement/cursor/ast.rs` | 49 | `sem/delete_statement/cursor.rs` |
| `sem/pg_type/type_ref/ast.rs` | 40 | `sem/pg_type/type_ref.rs` |
| `sem/pg_type/type_ref/canonicalize_error.rs` | 9 | `sem/pg_type/type_ref.rs` |
| `sem/scalar_expr/cast_expr/wildcard_projection.rs` | 23 | `sem/scalar_expr/cast_expr.rs` |
| `sem/scalar_expr/coalesce/wildcard_projection.rs` | 18 | `sem/scalar_expr/coalesce.rs` |
| `sem/scalar_expr/column_ref/wildcard_projection.rs` | 29 | `sem/scalar_expr/column_ref.rs` |
| `sem/scalar_expr/indirect/ast.rs` | 98 | `sem/scalar_expr/indirect.rs` |
| `sem/type_system/func_params/ddl.rs` | 54 | `sem/type_system/func_params.rs` |
| `sem/update_statement/cursor/ast.rs` | 62 | `sem/update_statement/cursor.rs` |
| `names/alias/ast.rs` | 11 | `names.rs` |
| `names/full/ast.rs` | 32 | `names/full.rs` |
| `names/ope/ast.rs` | 31 | `names/ope.rs` |
| `sem/alter_seq/ddl.rs` | 45 | `sem/alter_seq.rs` |
| `sem/alter_table/view.rs` | 39 | `sem/alter_table.rs` |
| `sem/column_rename/ddl.rs` | 25 | `sem/column_rename.rs` |
| `sem/comment/ddl.rs` | 28 | `sem.rs` |
| `sem/conflict_info/ast.rs` | 29 | `sem/conflict_info.rs` |
| `sem/create_comp_type/registration_error.rs` | 10 | `sem/create_comp_type.rs` |
| `sem/create_enum_type/ddl.rs` | 26 | `sem/create_enum_type.rs` |
| `sem/create_extension/ddl.rs` | 25 | `sem.rs` |
| `sem/create_schema/ddl.rs` | 62 | `sem/create_schema.rs` |
| `sem/create_table_as/ddl.rs` | 81 | `sem/create_table_as.rs` |
| `sem/delete_statement/cte.rs` | 20 | `sem/delete_statement.rs` |
| `sem/drop_dependency_behavior/ast.rs` | 10 | `sem.rs` |
| `sem/insert_statement/cte.rs` | 20 | `sem/insert_statement.rs` |
| `sem/merge_statement/cte.rs` | 20 | `sem/merge_statement.rs` |
| `sem/scalar_expr/ddl.rs` | 42 | `sem/scalar_expr.rs` |
| `sem/scalar_expr/wildcard_projection.rs` | 42 | `sem/scalar_expr.rs` |
| `sem/select_statement/cte.rs` | 20 | `sem/select_statement.rs` |
| `sem/update_statement/cte.rs` | 20 | `sem/update_statement.rs` |
| `sem/variable_set/ddl.rs` | 31 | `sem/variable_set.rs` |
| `sem/with_typmod/ast.rs` | 35 | `sem/with_typmod.rs` |
| `ast/ddl/check.rs` | 27 | `ast/ddl.rs` |
| `ast/ddl/column_attributes.rs` | 7 | `ast/ddl.rs` |
| `ast/ddl/serial.rs` | 67 | `ast/ddl.rs` |
| `ast/engine_features.rs` | 34 | `ast.rs` |
| `ast/ownership.rs` | 7 | `ast.rs` |
| `ast/wildcard.rs` | 89 | `ast.rs` |
| `sem/comment.rs` | 79 | `sem.rs` |
| `sem/error.rs` | 41 | `sem.rs` |
| `sem/constraint/names.rs` | 87 | `sem/constraint.rs` |
| `names/alias.rs` | 34 | `names.rs` |
| `sem/create_extension.rs` | 73 | `sem.rs` |
| `sem/drop_dependency_behavior.rs` | 33 | `sem.rs` |
| `sem/drop_rule.rs` | 89 | `sem.rs` |

The separate large-module relocation moved `sem/drop_rule/ast.rs` to
`sem/drop_rule_ast.rs`, keeping the implementation with the `DropRule` owner;
it is not counted as an absorption. At review planning, the worktree filesystem
had 78 GiB available, against an estimated 20 GiB of additional build/test use
and the repository 20 GiB minimum baseline. Capacity met the baseline.

### Relocated semantic test suites

The 11 former resolver test suites below are inline in the implementation
module that owns the conversion; no separate catch-all test module remains.

| Previous suite | Current implementation module | Owner / operation |
| --- | --- | --- |
| `ast_conversion/delete/tests.rs` | `sem/delete_statement/ast.rs` | `DeleteStatement::from_ast` |
| `ast_conversion/drop_rule/tests.rs` | `sem/drop_rule_ast.rs` | `DropRule::from_ast` |
| `ast_conversion/drop_trigger/tests.rs` | `sem/drop_trigger/ast.rs` | `DropTrigger::from_ast` |
| `ast_conversion/insert/tests.rs` | `sem/insert_statement/ast.rs` | `InsertStatement::from_ast` |
| `ast_conversion/merge/tests.rs` | `sem/merge_statement/ast.rs` | `MergeStatement::from_ast` |
| `ast_conversion/query/grouping_tests.rs` | `sem/select_statement/ast.rs` | `SelectStatement::from_ast_grouping` |
| `ast_conversion/query/tests.rs` | `sem/select_statement/ast.rs` | `SelectStatement::from_ast` |
| `ast_conversion/query/wildcard_tests.rs` | `sem/select_statement/result_column/ast.rs` | `ResultColumnCollection::ast_append_source_star` |
| `ast_conversion/rule/tests.rs` | `sem/create_rule/ast.rs` | `CreateRule::from_ast` |
| `ast_conversion/trigger/tests.rs` | `sem/create_trigger/ast.rs` | `CreateTrigger::from_ast` |
| `ast_conversion/update/tests.rs` | `sem/update_statement/ast.rs` | `UpdateStatement::from_ast` |

Three common-AST syntax synchronization tests remain inline under
`ast/public/wildcard.rs`. Semantic wildcard-projection implementations are in
`sem/scalar_expr.rs`, `sem/scalar_expr/cast_expr.rs`,
`sem/scalar_expr/coalesce.rs`, and `sem/scalar_expr/column_ref.rs`. The six
focused placement checks put operator-name, operator-resolution, SQLite-pattern,
PostgreSQL-pattern, ordering-operator, and name-resolution-phase tests beside
their owning conversion methods.
The source paths and baseline test counts below retain the names of all 106 old
Rust files.
`case attrs` is the sum of textual case-row definitions and parameter markers;
assertion counts are direct assertion macros inside test-attributed functions.

| Baseline source file | Test funcs / case attrs / assertions | Responsibility in current tree |
| --- | ---: | --- |
| `ast.rs` | 0 / 0 / 0 | AST node definitions remain; analysis dispatch is in `sem/sem_ast_analysis.rs` and raw parser entry points in `sem/top_level_statement.rs`. |
| `ast/ddl.rs` | 6 / 23 / 22 | AST syntax declarations remain here; semantic DDL conversion → `owner-specific semantic DDL module (physical map above)`. |
| `ast/ddl/alter_index.rs` | 0 / 0 / 0 | Semantic conversion/tests → `owner-specific semantic DDL module (physical map above)`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/ddl/check.rs` | 0 / 0 / 0 | Historical source path; syntax helper now lives in `ast/ddl.rs`; semantic conversion → `owner-specific semantic DDL module (physical map above)`. |
| `ast/ddl/column_attributes.rs` | 1 / 0 / 9 | Historical source path; syntax helper now lives in `ast/ddl.rs`; semantic conversion → `owner-specific semantic DDL module (physical map above)`. |
| `ast/ddl/column_rename.rs` | 1 / 0 / 3 | Semantic conversion/tests → `owner-specific semantic DDL module (physical map above)`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/ddl/serial.rs` | 5 / 24 / 20 | Historical source path; pure syntax helper now lives in `ast/ddl.rs`; semantic conversion → `owner-specific semantic DDL module (physical map above)`. |
| `ast/ddl/table_rename.rs` | 1 / 0 / 5 | Semantic conversion/tests → `owner-specific semantic DDL module (physical map above)`; syntax decoding is in the PostgreSQL AST adapter. |
| `ast/default_privileges.rs` | 2 / 13 / 12 | Default-privileges syntax remains; interpretation stays explicitly unimplemented in `sem/sem_ast_analysis.rs` (#955). |
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
| `ast/document.rs` | 0 / 0 / 0 | Common document syntax remains; semantic statement conversion is in `owner-specific semantic AST module (physical map above)`. |
| `ast/document/deparse.rs` | 0 / 0 / 0 | Syntax deparse remains at the same AST path. |
| `ast/document/merge.rs` | 0 / 0 / 0 | MERGE analysis → `owner-specific semantic AST module (physical map above)`; common AST document syntax remains in `ast/document.rs`. |
| `ast/document/merge/tests.rs` | 3 / 29 / 9 | MERGE tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/document/merge/tests/assignments.rs` | 10 / 33 / 28 | MERGE tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/document/merge/tests/integration.rs` | 8 / 0 / 21 | MERGE tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/document/merge/tests/views.rs` | 5 / 0 / 15 | MERGE tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/document/projection_walk.rs` | 0 / 0 / 0 | Syntax-only projection traversal remains; semantic wildcard rewrite consumes it. |
| `ast/document_validation.rs` | 2 / 4 / 2 | Structural AST validation remains at the same path. |
| `ast/drop_behavior.rs` | 0 / 0 / 0 | Syntax-only conversion remains; shared error is now `crate::error::AnalysisError`. |
| `ast/engine_features.rs` | 5 / 24 / 6 | Historical source path; syntax markers now live in `ast.rs`; semantic version/applicability checks → `sem/engine_features.rs`. |
| `ast/foreign_table.rs` | 2 / 12 / 12 | Foreign-table syntax remains; interpretation stays explicitly unimplemented in `sem/sem_ast_analysis.rs` (#956). |
| `ast/mutation.rs` | 0 / 0 / 0 | Public common-AST mutation operations remain at the same path. |
| `ast/ownership.rs` | 0 / 0 / 0 | Historical source path; ownership syntax declarations and the output-only helper now live in `ast.rs`; catalog lookup/application → `sem/ownership/ddl.rs`. |
| `ast/ownership/tests.rs` | 27 / 96 / 91 | Colocated tests → `sem/ownership/ddl.rs` test module. |
| `ast/postgres.rs` | 32 / 67 / 51 | PostgreSQL syntax conversion remains; four `test_name_from_range_var` cases → `ast_integration_tests`. |
| `ast/postgres/ddl.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing → `owner-specific semantic DDL module (physical map above)`. |
| `ast/postgres/document.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing is owned by the corresponding `owner-specific semantic AST module (physical map above)` family. |
| `ast/postgres/query.rs` | 0 / 0 / 0 | PostgreSQL-to-common-AST syntax conversion remains; semantic processing → `owner-specific semantic AST module (physical map above)`. |
| `ast/projection.rs` | 0 / 0 / 0 | Syntax-only projection editing remains; semantic wildcard rewrite uses it. |
| `ast/public.rs` | 18 / 30 / 52 | AST inspection API remains; `Statement::analyze` and its tests moved to `SemAst::from_ast` in `sem/sem_ast_analysis.rs`. |
| `ast/query_construction.rs` | 0 / 0 / 0 | Common-AST query construction remains at the same path. |
| `ast/resolve.rs` | 11 / 46 / 41 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete/tests.rs` | 17 / 40 / 49 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete/tests/cursors.rs` | 5 / 17 / 20 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete/tests/sql_driven.rs` | 1 / 0 / 13 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete/tests/versions.rs` | 3 / 10 / 9 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/delete/tests/views.rs` | 1 / 8 / 4 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/document.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/drop_rule.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/drop_rule/tests.rs` | 7 / 17 / 32 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/drop_trigger.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/drop_trigger/tests.rs` | 6 / 11 / 28 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/drop_trigger/tests/sqlite.rs` | 5 / 9 / 31 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/insert.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/insert/tests.rs` | 27 / 85 / 90 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/insert/tests/join_and_key_semantics.rs` | 2 / 15 / 11 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/insert/tests/views.rs` | 6 / 35 / 18 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/placement.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query.rs` | 1 / 6 / 2 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/clauses.rs` | 1 / 0 / 3 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/grouping_tests.rs` | 10 / 76 / 42 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests.rs` | 22 / 73 / 73 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/array_subquery.rs` | 10 / 27 / 35 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/call_arguments.rs` | 11 / 34 / 48 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/column_scope.rs` | 8 / 33 / 23 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/derived_aliases.rs` | 4 / 18 / 17 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/exists.rs` | 6 / 33 / 30 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/rows.rs` | 6 / 29 / 28 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/source_namespace.rs` | 22 / 97 / 69 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/table_functions.rs` | 21 / 77 / 69 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/tests/whole_rows.rs` | 26 / 69 / 90 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/query/wildcard_tests.rs` | 21 / 54 / 71 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/returning.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/rule.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/rule/tests.rs` | 13 / 60 / 49 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/trigger.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/trigger/tests.rs` | 16 / 59 / 61 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/update.rs` | 0 / 0 / 0 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/update/tests.rs` | 13 / 40 / 39 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/update/tests/assignments.rs` | 1 / 0 / 2 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/update/tests/cursors.rs` | 4 / 17 / 16 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/resolve/update/tests/views.rs` | 1 / 9 / 2 | Semantic implementation and tests → `owner-specific semantic AST module (physical map above)`. |
| `ast/returning_options.rs` | 0 / 0 / 0 | Syntax options remain; RETURNING analysis → `owner-specific semantic AST module (physical map above)`. |
| `ast/rows_from.rs` | 0 / 0 / 0 | Syntax constructors remain; query/FROM analysis → `owner-specific semantic AST module (physical map above)`. |
| `ast/rule.rs` | 17 / 42 / 32 | Rule syntax construction remains; semantic conversion → `owner-specific semantic AST module (physical map above)`. |
| `ast/rule/tests.rs` | 6 / 11 / 14 | Dialect-syntax conversion tests remain with their AST implementation. |
| `ast/sqlite.rs` | 1 / 6 / 4 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `owner-specific semantic AST module (physical map above)`. |
| `ast/sqlite/ddl.rs` | 0 / 0 / 0 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `owner-specific semantic AST module (physical map above)`. |
| `ast/sqlite/dml.rs` | 4 / 0 / 27 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `owner-specific semantic AST module (physical map above)`. |
| `ast/sqlite/query.rs` | 0 / 0 / 0 | SQLite syntax conversion remains in AST; semantic dispatch/resolution is in `sem/top_level_statement.rs` and `owner-specific semantic AST module (physical map above)`. |
| `ast/sqlite_retention.rs` | 2 / 3 / 4 | SQLite syntax-retention logic remains at the same AST path. |
| `ast/tests.rs` | 24 / 109 / 51 | Cross-component tests → crate-root `ast_integration_tests.rs`. |
| `ast/tests/sqlite_from_groups.rs` | 3 / 37 / 9 | Cross-component tests → crate-root `ast_integration_tests/sqlite_from_groups.rs`. |
| `ast/tests/stack_regressions.rs` | 2 / 18 / 0 | Cross-component tests → crate-root `ast_integration_tests/stack_regressions.rs`. |
| `ast/traversal.rs` | 1 / 0 / 2 | Syntax-tree traversal remains at the same path. |
| `ast/trigger.rs` | 16 / 50 / 54 | Trigger syntax construction remains; semantic conversion → `owner-specific semantic AST module (physical map above)`. |
| `ast/trigger/tests.rs` | 3 / 5 / 33 | Dialect-syntax conversion tests remain with their AST implementation. |
| `ast/utility.rs` | 2 / 0 / 8 | Syntax-only AST utility operations remain at the same path. |
| `ast/validation.rs` | 1 / 4 / 1 | Structural AST validation remains; stack handling calls the crate stack directly. |
| `ast/view_definition.rs` | 0 / 0 / 0 | View lineage and semantic conversion → `semantic view-owner module (physical map above)`; source file removed. |
| `ast/wildcard.rs` | 0 / 0 / 0 | Historical source path; AST wildcard helper merged into `ast.rs`; syntax synchronization tests remain in `ast/public/wildcard.rs` and semantic expansion is in `sem/wildcard_expansion/rewrite.rs`. |
| `ast/wildcard/rows_source.rs` | 4 / 6 / 13 | Rewrite consumer and tests → `sem/wildcard_expansion/rewrite/rows_source.rs`. |
| `ast/wildcard/source.rs` | 0 / 0 / 0 | Rewrite consumer and tests → `sem/wildcard_expansion/rewrite/source.rs`. |
| `ast/wildcard/tests.rs` | 16 / 46 / 44 | Inline rewrite tests → `sem/wildcard_expansion/rewrite.rs`; syntax synchronization tests → `ast/public/wildcard.rs`. |

## Compiled-case preservation status

Round-two compiled comparisons and standard-flow results below are historical:
they predate the round-three module consolidation and do not establish current
worktree results. Round-three compiled comparisons and both full standard
flows have now passed. Against `accb7d65`, the default comparison has
4,363 unchanged and 18 relocated cases (4,381 total); all features has 5,090
unchanged and
18 relocated cases (5,108 total). Both contain 28 suites, with no missing or
extra cases, identity collisions, or metadata changes. Their normalized
identity SHA-256 values are respectively
`3fd00dadea75573719a2e45a801aafa12fc1fb229f3d4c05248c6e0c42fd441e` and
`1bdf40c74266b57206977cfe173d64aec1d994fd723c66ba8a245e0e0f9a2978`.

The full default flow passed 4,381 SQLAST and 5,475 workspace cases; the full
all-features flow passed 5,108 SQLAST and 6,254 workspace cases. Both retained
63 existing ignored workspace cases and passed all 3,064 SQL regression cases
in 166 groups. Feature boundaries, formatting, Clippy, import lint, Markdown,
documentation, doctests, and the other standard checks passed. No SLOW warning
or timeout occurred. Packaging and SemVer/release-intent checks passed for
implementation commit `a9e828cb038da8ee1ffaf8253f922fa5e098ead0`. Package
layout, metadata, checksums, and executable identity were verified. Executables
extracted from that commit's generated archive passed all 3,064 SQL regression
cases in 166 groups. Package versions and release intents are unchanged.

The round-three source audit at `accb7d65` maps all 1,328 test functions to
1,328 destinations with identical bodies, cases, attributes, parameter
signatures, and effective cfg. It reports no unmapped or new tests, attribute
or cfg differences, or missing module targets. Eight tests moved inside two
focused inline modules; the two unattached files are upstream SQLite parser
sources outside the migrated test set. The audit also found all 9,434
production and test functions: 9,431 bodies/cfg matched exactly and three
alias-only differences were reviewed. All 988 model declarations and adjacent
attributes are preserved. Production and test cfg gates were audited. There are
no unexplained import, cfg, allow, or expect changes, no new production inline
namespace modules, and no remaining eligible small modules. The only new
public path is `sem::ProblemLikeError`; its `pub` declaration keeps the
existing public `ValidOperatorName::try_from` return type nameable after moving
the type out of private `sem/error`. The method signature and error variants
are unchanged. These source audits do not replace compiled-case or behavioral
validation.

The earlier default/all-features comparisons and standard-flow results predate
the current physical-module and syntax-import corrections. The round-two
default comparison against correction baseline
`0d4fe0edfdf04eaed0f9ecbaab43b8e113386df5` reconciles 4,381 cases across the
same 28 suites: 3,215 identities are unchanged and 1,166 are relocated. It
found no missing or extra identities or metadata changes; the normalized
identity SHA-256 is
`bfc4b2d15afa24c7e242bc02ed4d20293ee2b3447e88f6368ca9dda59195e19e`. The 12
default-profile cursor cases are included.

The round-two all-features comparison against the same correction baseline
reconciles 5,108 cases across 28 suites: 3,794 identities are unchanged and
1,314 are relocated. It found no missing or extra identities or metadata
changes; the normalized identity SHA-256 is
`073515cc291b6d350fcaf32cd0ffcec5ab8f19c2a3772ec7568ee699bde15bd7`.

Both historical round-two standard flows passed. Default passed 4,381
`sql-fun-sqlast` tests, 5,475 workspace tests with 63 existing ignored tests,
and 3,064 SQL
regression inputs. All-features passed 5,108 `sql-fun-sqlast` tests, 6,254
workspace tests with 63 existing ignored tests, and 3,064 SQL regression inputs
in 166 groups. Neither flow reported SLOW warnings or timeouts.

### Source assertion and case-count evidence

The resolver and MERGE source ledger covers 47 old files: 334 attributed test
functions, 1,166 `#[case]` attributes, and 1,188 assertion macros inside those
attributed test bodies. A whole-file assertion-macro regex yields 1,198 hits;
the extra 10 are in `ast/resolve/query/tests/column_scope.rs` (3),
`ast/resolve/query/tests/exists.rs` (1), and
`ast/resolve/query/wildcard_tests.rs` (6), outside the attributed function
bodies. These are source counts, not compiled-case totals. Current physical
destinations are listed above and do not follow a path-preserving resolver
layout.

The DDL/ownership/view/engine ledger covers 11 old files. Its before/after
source counts match: 21 `#[test]`, 25 `#[rstest::rstest]`, 167 `#[case]`, two
`#[should_panic]`, and 156 assertion macros. Ownership test bodies are inlined
under `sem/ownership/ddl.rs`; CHECK and SERIAL syntax helpers remain in AST
while their semantic conversion and catalog effects live in the physical DDL
modules above. These are source-preservation facts, not compiled-case counts.

The API migrations intentionally change caller expressions
(`statement.analyze(...)` to `SemAst::from_ast(...)`, and the `Statement`
wildcard method to `WildcardExpansion::rewrite(...)`). Source comparisons
record these authorized migrations and their limits. Default and
all-features compiled identities are summarized above.

## Physical relocation and source-preservation evidence

The round-two source comparison uses correction baseline
`0d4fe0edfdf04eaed0f9ecbaab43b8e113386df5`; it is historical for the current
after-consolidation tree:

- The 47 expression/query source files retain 334 test functions, 1,166 case
  attributes, 16 value dimensions, 1,198 assertion-macro calls, 10 TODOs, and
  50 feature gates. The per-file counts match before and after. Raw relocation
  preserves all original function-body token multisets and test-attribute
  multisets before rustfmt.
- The 13 DDL/peripheral source files map 120 implementation items into 42
  implementation destinations (43 destination files including colocated tests).
  The source preservation report has no differences:
  32 `#[test]` functions, 36 rstest functions, 205 case attributes, one values
  attribute, four panic attributes, 210 assertion calls, 24 TODOs, and feature
  gates match. Two identical SQLite-gated test imports were consolidated into
  one equally gated import group; test bodies and effective test gates are
  unchanged.
- Round two mapped 1,328 source test functions to 1,328 destinations. It
  reported no missing/new tests, attribute changes, effective cfg changes, or
  missing module targets. 1,318 function bodies matched exactly; ten body-token
  differences were reviewed as equivalent after relocation and formatting.
  Round three has a separate exact test-identity map, summarized above.
- The round-two placement audit found all 37 relocated test modules inline
  beside an implementation/function or in focused modules testing
  `SemAst::from_ast`; none lacked an inline destination. The 11 resolver suites
  above are part of that historical set. Two upstream SQLite parser files are
  outside the migration's test-source set.
- A model audit matched all 988 struct, enum, and union definitions plus their
  adjacent attributes, including derive and serde attributes; it found no
  changed, removed, or added definitions. Global imports were normalized in
  143 files, with no type or schema change. The AST-options export audit found
  ten AST exports, zero semantic exports, and zero semantic imports of these
  types.

The round-two source reports preserve source attributes, test identities,
assertions, and reviewed body equivalence. The round-three source audit confirms
current test bodies, attributes, signatures, and effective cfg, with the
reviewed alias-only function differences summarized above. These audits do not
establish semantic behavior or replace the compiled reconciliations and
standard flows. No raw assertion text is copied into this inventory.

## Existing consumers and regression inventory

- The server analyzes edited statements through `SemAst::from_ast` in
  `sql-fun-server/src/session/query_analysis.rs` and applies wildcard expansions
  through `WildcardExpansion::rewrite`. Its cache context remains caller-owned
  and is
  reused across statements.
- Existing direct-analysis examples are colocated in
  `sem/sem_ast_analysis.rs`; callers also exist in parameter-resolution
  tests and crate `test_helpers`. No semantic analysis example remains attached
  to the AST `Statement` API.
- The SQLAST CLI regression manifest at the base has 85 fixture rows expanding
  to 166 fixture/version groups. Every manifest case count matches its JSON
  fixture length. Manifest-derived inputs are source inventory counts; both
  round-two standard flows executed 3,064 regression inputs as recorded above.

## Review correction and validation status

The first PR revision (`22fca6e0`) did not meet the implementation-ownership
requirement. The maintainer required eliminating the generic source-typed
conversion adapter and putting wildcard rewriting on `WildcardExpansion`.
These are explicit review requirements, not additional feature scope. The
current physical layout moves implementations to defining modules and their
descendants; no SQL behavior, semantic serialization, runtime dependency, or
contributor workflow change is intended.

The current conversion index and physical maps reflect these corrections:

- All 318 expression/query implementation blocks and 120 DDL items are in
  the defining type module or a descendant. The former `ast_conversion` and
  `ddl_conversion` catch-all modules are removed. The independent owner audit
  found zero ownership violations after consolidation.
- Table-function output name rules are owned by `TableFunctionOutput`.
  `OperatorExpr::table_output_name(&self)` maps to the constant
  `TableFunctionOutput::nullif_output_name()` and takes no AST argument.
  `TypedColumns::resolve_function_output` is replaced by
  `TableFunctionOutput::from_ast_typed_columns` in
  `sem/data_source/function_columns.rs`; callers pass the original AST list.
- `FuncCallExpr` AST implementations are in `sem/scalar_expr/func_call.rs`;
  statement-specific CTE helpers are in their statement owner files, while
  common CTE conversion remains in `sem/cte/ast.rs`. ALTER TABLE conversion is
  in `sem/alter_table/ddl.rs`; view alteration remains with `AlterTable`.
- `SERIAL` syntax expansion remains the pure `ast/ddl.rs` helper and is
  applied only to a temporary cloned column. Semantic resolution, validation,
  and sequence catalog effects are in `sem/create_table/serial.rs`.
- Syntax-valued option definitions and parser conversions are owned by
  `ast/options.rs`, with rendering in its `deparse` child. The former `sem`
  re-export aliases for the ten AST syntax option types are removed; workspace
  callers import the AST types directly. Type definitions and serialization
  remain unchanged.
- Shared `AnalysisError` remains in root `src/error.rs`, including the
  syntax-level dialect check. Semantic error helpers are colocated with
  `CreateCompositeType` and `TypeReference` in their owner files.
- Semantic wildcard-projection implementations now live in their owner files:
  `sem/scalar_expr.rs`, `sem/scalar_expr/cast_expr.rs`,
  `sem/scalar_expr/coalesce.rs`, and `sem/scalar_expr/column_ref.rs`.
  `WildcardExpansion` continues to own staged statement rewriting. Three syntax
  synchronization tests remain inline under `ast/public/wildcard.rs`.
- `ast/sqlite.rs` remains the AST-side SQLite syntax adapter. Semantic analysis
  now resides only with the relevant semantic owner types.

Round-two compiled reconciliations, standard flows, packaging, SemVer, and
installed-distribution checks are historical evidence only. Round-three compiled
reconciliations and both full standard flows passed as recorded above.
Packaging, SemVer, and installed checks also passed for `a9e828cb`, as
recorded above. The final follow-up commit only records validation and wraps
prose; the validated Rust sources are unchanged. Source identity, model
preservation, and ownership maps alone do not establish semantic behavior or
current
validation.

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
`sem/data_source/function_columns.rs`. It consumes the original syntax
list directly; no borrowed `TypedColumns` adapter remains. The NULLIF
output-name helper is a constant and accepts no AST node because its caller
already selected that semantic operation.
