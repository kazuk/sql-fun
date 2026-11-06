# Semantic parser-boundary inventory (#1005)

Baseline: `e087bf0652919bed0650cd2a01f104cf417eec01`. The line references
below identify the original search matches, including documentation and test
code; they are not a count of independent dependencies. The audit also checked
parser aliases, public signatures, trait implementations, `TryFromWithTokens`,
feature-gated branches and consumers outside `sem`. The baseline test inventory
was compiled and listed; baseline test execution was not performed.

## Dispositions

| Category | Original use and disposition | Consumers and gates |
| --- | --- | --- |
| Dispatch / DDL | PostgreSQL/SQLite parser-node dispatch and public mixed-stage entry points removed. AST conversion occurs before `SemAst::from_ast`; SQLite schema loading retains its prior unsupported-statement boundary. | Schema snapshot and SQLite loaders, server and integration helpers; SQLite branches gated by `sqlite`. |
| DML / query | UPDATE, INSERT, DELETE, MERGE, SELECT and CTE node adapters removed; common AST is analyzed by the existing destination-owned methods. | Sem callers, integration tests and schema loaders; PostgreSQL and SQLite where supported. |
| DDL / constraints | Parser-node adapters, constraint trait and parser-derived semantic values removed; AST conversion feeds existing constraint, table and alteration analysis. `CreateTableAs` now stores semantic `Kind`. | DDL and schema catalog operations, associated unit tests; SQLite code gated by `sqlite`. |
| Expressions / tests | Scalar, operator, array, cast and expression adapters removed. Semantic tests consume AST; unavoidable synthetic/parser construction resides in crate-root test fixtures. | Scalar resolution and wildcard tests; SQLite cast branches gated by `sqlite`. |
| Sources / query / tests | Range, join, function, alias and FROM adapters removed. Tests use common AST or fixture construction; parser-only validation lives in the test fixture. | Query and table-function resolution, wildcard rewriting, SQLite feature tests. |
| Function parameters | Parser-parameter adapters and legacy output-parameter helpers removed; common AST parameter modes drive existing semantic handling. | Function creation and inline tests; PostgreSQL. |
| Name / operator | Parser conversions moved to `ast`; canonical name and operator operations remain on their owning `names` types. Synthetic malformed input is prepared outside `sem`. | All name and operator consumers; PostgreSQL plus SQLite-gated parser tests. |
| Diagnostic conversion | Parser-specific operator/type diagnostic conversion removed from `sem`; AST adapters retain syntax conversion before semantic diagnostics. | PostgreSQL expression/type diagnostics. |
| Hint scanner / grammar | PostgreSQL scanner, SQLite lexer, and shared hint grammar moved to `ast/client_type`; the result is neutral hint syntax. Semantic code validates modifiers and resolves type identity. | Client parameter hints; SQLite lexer gated by `sqlite`. |

The final semantic modules contain no direct `syn`, `ScanToken`,
`TryFromWithTokens`, `pg_query` or `turso_parser` references. No diagnostic-only
`ScanToken` exception remains. The test-only fixtures still parse SQL or build
synthetic parser nodes where those inputs are essential; they return common AST
values to semantic tests. For fabricated non-string operator nodes, the fixture
checks that AST conversion rejects the original node, then checks semantic
operator diagnostics using the corresponding invalid common-name shape.
`ParseContext::resolve_table_function` was an unused parser-specific shortcut
and was removed; the AST-driven source path remains.
An indirect-call audit also found the parser-only negative test in
`sem/alter_index/tests.rs`; its parser call now lives in the crate-root fixture.
The `Name` conversion from PostgreSQL `RangeVar` is now fallible to preserve
the original catalog-without-schema rejection for synthetic nodes. PostgreSQL
17 and 18 reject the corresponding `db..items` spelling.

## Baseline occurrence ledger

Paths are relative to `sql-fun-sqlast/src`. Lines refer to the baseline and
include both production and test occurrences. Each row is covered by the
corresponding disposition above; individual matches include imports, usages,
signatures, feature-gated tests and rustdoc references.

| Baseline file | Lines | Disposition |
| --- | --- | --- |
| `sem.rs` | 127, 132, 137, 653, 659, 663, 700, 703, 709, 778, 808, 812, 818 | Dispatch / DDL |
| `sem/alter_owner.rs` | 3, 6 | DDL / constraints |
| `sem/alter_seq.rs` | 2, 8, 11, 36, 40, 45, 62, 66, 72 | DDL / constraints |
| `sem/alter_table.rs` | 6, 9, 17, 165, 169, 170, 177, 183, 184, 188, 189, 196, 202, 204, 207, 215, 236, 237, 242, 248, 392, 426 | DDL / constraints |
| `sem/alter_table/ddl.rs` | 210, 622 | DDL / constraints |
| `sem/client_type/token.rs` | 381 | Hint scanner / grammar |
| `sem/column_constraints.rs` | 32, 37, 38, 563, 564, 570, 579, 585, 592 | DDL / constraints |
| `sem/column_constraints/attr_constraint.rs` | 7, 54, 59, 64, 138, 143, 148, 218, 223, 228 | DDL / constraints |
| `sem/column_constraints/check_constraint.rs` | 5, 8, 15, 133, 138, 139, 145, 152, 158, 164 | DDL / constraints |
| `sem/column_constraints/collate_constraint.rs` | 2, 66, 71, 77, 130 | DDL / constraints |
| `sem/column_constraints/default_constraint.rs` | 2, 10, 72, 77, 78, 83, 93, 110, 115, 124 | DDL / constraints |
| `sem/column_constraints/exclusion_constraint.rs` | 7, 53, 58, 63 | DDL / constraints |
| `sem/column_constraints/foreign_key_constraint.rs` | 8, 13, 159, 164, 169, 178, 180, 186, 202, 203, 204, 205, 273, 278, 283, 288, 316, 341 | DDL / constraints |
| `sem/column_constraints/generated_constraint.rs` | 3, 12, 80, 85, 86, 91, 100, 106, 123, 128, 137, 141 | DDL / constraints |
| `sem/column_constraints/identity_constraint.rs` | 8, 64, 69, 74, 83 | DDL / constraints |
| `sem/column_constraints/nullability_constraint.rs` | 2, 10, 86, 91, 96, 108, 117, 122, 131, 132 | DDL / constraints |
| `sem/column_constraints/primary_key_constraint.rs` | 2, 10, 64, 69, 74, 88, 97, 102, 110 | DDL / constraints |
| `sem/column_constraints/unique_constraint.rs` | 2, 10, 61, 66, 71, 86, 91, 100 | DDL / constraints |
| `sem/constraint.rs` | 14, 23, 28, 36, 190 | DDL / constraints |
| `sem/constraint/check.rs` | 4, 14, 279 | DDL / constraints |
| `sem/constraint/foreign.rs` | 8 | DDL / constraints |
| `sem/constraint/primary.rs` | 1, 10 | DDL / constraints |
| `sem/constraint/unique.rs` | 1, 10 | DDL / constraints |
| `sem/create_comp_type.rs` | 11, 60, 139, 140, 144, 149, 151 | DDL / constraints |
| `sem/create_comp_type/ddl.rs` | 77, 80 | DDL / constraints |
| `sem/create_domain.rs` | 3, 10, 14, 145, 148, 149, 154 | DDL / constraints |
| `sem/create_domain/ddl.rs` | 67 | DDL / constraints |
| `sem/create_enum_type.rs` | 7, 10, 70 | DDL / constraints |
| `sem/create_function.rs` | 3, 10, 14, 107, 111, 112, 117 | DDL / constraints |
| `sem/create_function/ddl.rs` | 125, 127 | DDL / constraints |
| `sem/create_index.rs` | 8, 10, 12, 19, 37, 190, 434, 449, 453, 454, 459 | DDL / constraints |
| `sem/create_index/ddl.rs` | 60, 73, 103, 197, 223 | DDL / constraints |
| `sem/create_schema.rs` | 3, 5, 9, 52, 55, 56, 62 | DDL / constraints |
| `sem/create_seq.rs` | 8, 11, 121, 125, 130 | DDL / constraints |
| `sem/create_table.rs` | 7, 12, 25, 917, 923, 943, 951, 958, 966, 974, 982, 991, 996, 1003, 1057, 1058, 1064, 1065, 1068, 1073, 1074, 1087, 1100, 1106, 1107 | DDL / constraints |
| `sem/create_table/column_def.rs` | 3, 5, 13, 374, 379, 391, 396, 402 | DDL / constraints |
| `sem/create_table/column_def/ddl.rs` | 183 | DDL / constraints |
| `sem/create_table/ddl.rs` | 101, 298, 458, 500 | DDL / constraints |
| `sem/create_table_as.rs` | 3, 14, 18, 133, 138, 139, 146 | DDL / constraints |
| `sem/create_view.rs` | 12, 16, 260, 265, 266, 271, 274, 276 | DDL / constraints |
| `sem/cte.rs` | 3, 12, 163, 222, 225, 226, 231 | Sources / query / tests |
| `sem/data_source.rs` | 459, 461, 472 | Sources / query / tests |
| `sem/data_source/alias.rs` | 5, 92, 125 | Sources / query / tests |
| `sem/data_source/ast.rs` | 19, 20, 31, 54, 72, 90, 108, 599, 669 | Sources / query / tests |
| `sem/data_source/join_kind.rs` | 4 | Sources / query / tests |
| `sem/data_source/resolved.rs` | 23, 130, 147, 157, 179 | Sources / query / tests |
| `sem/data_source/rows_fun.rs` | 2, 15, 298, 318 | Sources / query / tests |
| `sem/data_source/undefined.rs` | 8 | Sources / query / tests |
| `sem/declaration_collation.rs` | 252 | DDL / constraints |
| `sem/def_elem.rs` | 1, 6, 89, 92, 97 | DDL / constraints |
| `sem/delete_statement.rs` | 15, 22, 186, 189 | DML / query |
| `sem/engine_features.rs` | 373, 375 | Sources / query / tests |
| `sem/from_clause.rs` | 13, 362, 366, 367, 372 | Sources / query / tests |
| `sem/insert_statement.rs` | 13, 22, 717, 723 | DML / query |
| `sem/merge_statement.rs` | 18, 471, 473, 475 | DML / query |
| `sem/name_conversion.rs` | 4, 11, 192, 295, 307, 322, 363, 395, 501 | Name / operator |
| `sem/problem.rs` | 21, 116 | Diagnostic conversion |
| `sem/relation_alteration.rs` | 75 | DDL / constraints |
| `sem/scalar_expr/analyze.rs` | 4, 11, 13, 19, 23, 24, 29, 38, 42, 48, 54, 70, 86, 102, 118, 134, 154, 160 | Expressions / tests |
| `sem/scalar_expr/arith_expr/call_sites/binary.rs` | 10 | Expressions / tests |
| `sem/scalar_expr/arith_expr/call_sites/binary_all.rs` | 10 | Expressions / tests |
| `sem/scalar_expr/arith_expr/call_sites/binary_any.rs` | 10 | Expressions / tests |
| `sem/scalar_expr/boolean.rs` | 10 | Expressions / tests |
| `sem/scalar_expr/cast_expr.rs` | 2, 140, 144, 150 | Expressions / tests |
| `sem/scalar_expr/context.rs` | 1487, 1501, 1507, 1529, 1534 | Expressions / tests |
| `sem/scalar_expr/func_call/implementation/analysis.rs` | 108 | Expressions / tests |
| `sem/scalar_expr/indirect/array_index.rs` | 9, 99 | Expressions / tests |
| `sem/scalar_expr/indirect/array_slice.rs` | 8, 94, 112 | Expressions / tests |
| `sem/select_statement.rs` | 12, 21, 171, 174, 175, 180, 252, 255, 256, 261 | DML / query |
| `sem/top_level_statement.rs` | 3, 6, 9, 30, 31, 36, 46, 73, 89, 101 | Dispatch / DDL |
| `sem/type_system/func_params.rs` | 27, 57, 498 | Function parameters |
| `sem/type_system/func_params/analyze.rs` | 2, 5, 18, 31, 60 | Function parameters |
| `sem/type_system/func_params/in_out_param.rs` | 7, 19, 147 | Function parameters |
| `sem/type_system/func_params/in_param.rs` | 1, 11, 108 | Function parameters |
| `sem/type_system/func_params/out_param.rs` | 4, 16, 110 | Function parameters |
| `sem/type_system/func_params/table_param.rs` | 4, 16, 137 | Function parameters |
| `sem/type_system/func_params/variadic_param.rs` | 1, 10, 92 | Function parameters |
| `sem/update_statement.rs` | 10, 18, 417, 420 | DML / query |
| `sem/variable_set.rs` | 5, 43, 44, 49, 50, 82, 83, 88 | DDL / constraints |

## Test reconciliation

`cargo nextest list -p sql-fun-sqlast` yielded 4,381 cases before and after;
`--all-features` yielded 5,108 before and after. After the module-prefix mapping
below, the complete case identities, including named parameter cases, match
exactly in each configuration. Assertions for removed parser-only helper APIs
were moved to their corresponding AST or semantic boundary. No fixture manifest
was changed: 85 fixture rows, 1,561 cases and 3,064 versioned cases across 166
fixture/version groups. The maintained manifest and version selectors retain
PostgreSQL 17/18 coverage.

| Previous test module | Current test module |
| --- | --- |
| `sem::column_constraints::foreign_key_constraint::{test_analyze_constraint,test_analyze_constraint_sqlite}` | `sem::column_constraints::ddl::{test_analyze_constraint,test_analyze_constraint_sqlite}` |
| `sem::column_constraints::collate_constraint::test_analyze_sqlite_column_definition_reports_unknown_collation` | `sem::create_table::column_def::ddl::test_analyze_sqlite_column_definition_reports_unknown_collation` |
| `sem::constraint::foreign::test_analyze` | `sem::constraint::ddl::test_analyze` |
| `sem::constraint::test_analyze_alter_obj_add_constraint` | `sem::constraint::ddl::test_analyze_alter_obj_add_constraint` |
| `sem::name_conversion::canonical_parse_tests` | `names::full::canonical_parse_tests` |
| `sem::name_conversion::{operator_conversion_tests,operator_reporting_tests}` | `names::ope::{operator_conversion_tests,operator_reporting_tests}` |
| `sem::name_conversion::{range_var_tests,sql_name_conversion_tests,tests}` | `ast::postgres::{range_var_tests,sql_name_conversion_tests,tests}` |
| `sem::top_level_statement::tests` | `sem::sem_ast_analysis::tests` |
| `sem::type_system::func_params::analyze::test_function_param_analyze_as_column_definition` | `sem::type_system::func_params::test_function_param_analyze_as_column_definition` |
