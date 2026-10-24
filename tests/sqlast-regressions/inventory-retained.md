# Builtin-dependent SQL AST test inventory

This inventory covers builtin-dependent tests in `sql-fun-sqlast/src` and
`sql-fun-sqlast/tests`, including real-catalog fixtures, `TestParseContext`,
and helpers that install synthetic type, cast, operator, or function data.
`graphify-out/graph.json` was absent in this worktree, so the inventory used
source searches and followed the test helpers directly.

`TestParseContext::setup_type` is not a production builtin fixture: it registers
`TypeDefinition::builtin` and adds only an identity/no-conversion cast.
`setup_binary`, `setup_function`, and the cast/operator setters provide
hand-authored lookup results. Tests using those helpers are listed separately
from assertions that exercise shipped PostgreSQL metadata. A mention of
`PgBuiltInType`, a builtin spelling, or a real catalog in setup does not alone
make a test builtin-dependent; the assertion must rely on builtin resolution,
cast, overload, or type behavior.

## Coordinator-owned integration groups

These groups have separate fixture allocations or detailed per-case disposition
in the linked inventory. Their presence here ensures the builtin-dependent
integration scope is visible without duplicating each fixture row.

| Integration group | Coverage and disposition |
| --- | --- |
| [`arrays.rs`](../../sql-fun-sqlast/tests/arrays.rs) | Constructor common types, casts, parameter alternatives, nested arrays, and record overloads map to the `arrays-*.json` fixtures in [`manifest.tsv`](manifest.tsv). |
| [`min_max.rs`](../../sql-fun-sqlast/tests/min_max.rs) | GREATEST/LEAST result selection, ordered casts, invalid operands, domains, and parameter domains map to the `minmax-*.json` fixtures in [`manifest.tsv`](manifest.tsv). |
| [`case_parameters.rs`](../../sql-fun-sqlast/tests/case_parameters.rs) and [`coalesce_parameters.rs`](../../sql-fun-sqlast/tests/coalesce_parameters.rs) | Retain real-catalog correlated parameter domains, result-column selections, comparison/operand AST bindings, and cast metadata. Separate error-only cases are mapped in [`inventory-expressions.md`](inventory-expressions.md); cases whose CLI output omits a source-required diagnostic remain Rust-only there. |
| [`query_resolution.rs`](../../sql-fun-sqlast/tests/query_resolution.rs) | Per-case public resolver success/failure disposition is recorded in [`inventory-query.md`](inventory-query.md). |
| [`type_metadata_regressions.rs`](../../sql-fun-sqlast/tests/type_metadata_regressions.rs) | Qualified domain identity, preferred cast priority, and concrete/parameter overload selection are covered by the metadata fixture where applicable; custom-schema cases stay in Rust under the accepted builtin-only scope. |
| [`writable_views.rs`](../../sql-fun-sqlast/tests/writable_views.rs) | Per-case dispositions are in [`inventory-query.md`](inventory-query.md). Custom project-schema scenarios remain Rust coverage under #727; many cases inspect view-write support, binding/lineage, applicability, and catalog-layer state that the CLI result contract does not expose. |

## Other real-catalog semantic coverage

These groups add real-catalog consumer assertions beyond the coordinator-owned
integration groups above.

| File/group | Builtin-dependent assertion | Disposition |
| --- | --- | --- |
| [`parameter_resolution.rs`](../../sql-fun-sqlast/tests/parameter_resolution.rs) | Numeric source constraints from real operators/casts; parser adapter parity; enum I/O casts and nested parameter domains. | Retain: assertions inspect domain `allows`/correlation, parser-adapter equivalence, enum explicit-cast APIs, and serialized nested constraints. The CLI contract cannot inspect those internal domains or bindings. |
| [`domain_casts.rs`](../../sql-fun-sqlast/tests/domain_casts.rs) | Explicit and I/O cast availability, domain identity, array/domain coercions, and parameter input domains. | Retain internal target lookup, cast argument/resolution structure, and domain input/output constraints. Custom-schema cases remain Rust coverage under the accepted builtin-only scope. |
| [`scalar_any.rs`](../../sql-fun-sqlast/tests/scalar_any.rs), [`scalar_all.rs`](../../sql-fun-sqlast/tests/scalar_all.rs) | Actual equality overloads and cast families, invalid operator cases, and shipped catalog clauses. | Retain operator/operand binding assertions, serialized semantic-tree checks, and scans of shipped catalog view bindings. |
| [`variadic_any.rs`](../../sql-fun-sqlast/tests/variadic_any.rs) | Imported `pg_catalog.format` overload binding through the shipped declaration and builtin views. | Retain selected-overload, SQL argument position, argument binding, and serialized binding assertions. |
| [`vectors.rs`](../../sql-fun-sqlast/tests/vectors.rs) | Vector array/cast behavior, polymorphic function signatures, operator bindings, and conversion contexts. | The four `invalid_vector_inputs` cases have been migrated by the coordinator to `vectors.json` (PostgreSQL 18). Other vector groups remain in Rust because they inspect catalog conversion context, semantic bindings, or structure beyond the CLI result contract. |
| [`whole_row_catalog.rs`](../../sql-fun-sqlast/tests/whole_row_catalog.rs) | The real `_pg_truetypid` overload and shipped catalog view path with whole-row arguments. | Retain function-overload, typed whole-row argument, and relation-binding assertions. |
| [`schema_context_chain.rs`](../../sql-fun-sqlast/tests/schema_context_chain.rs) — `inherited_catalog_objects_and_merged_overloads_drive_query_analysis` | Real inherited catalog objects and merged builtin overloads determine expression types. | Retain: asserts inherited context layers, visible objects, and merged overloads through the internal layer API. |
| [`schema_context_chain.rs`](../../sql-fun-sqlast/tests/schema_context_chain.rs) — `sql_casts_preserve_array_identity` | SQL cast identity and array subscripting. | Migrated by the coordinator to `casts.json`: five cast inputs plus four indexed subscripting inputs, verified against PostgreSQL 17 and 18; the superseded Rust group was removed. |

## CASE and COALESCE semantic-only cases

The source tests in
[`parameter_resolution/case_tests.rs`](../../sql-fun-sqlast/src/sem/parameter_resolution/case_tests.rs)
and [`scalar_expr/coalesce/tests.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/coalesce/tests.rs)
use a small synthetic catalog. The public-boundary fixtures capture CLI
diagnostics and column metadata under the builtin catalog; they do not replace
the source tests' synthetic-context constraint and AST assertions:

| Fixture | Source case | Public contract asserted |
| --- | --- | --- |
| [`case_semantic.json`](case_semantic.json) | CASE parameter conflicts and an invalid searched condition. | `parameter_constraint`, the CLI's `parameter_type_unresolved` result for the non-boolean condition case, and result-column nullability/type absence. |
| [`coalesce_semantic.json`](coalesce_semantic.json) | Incompatible concrete operands, parameter conflicts, and unreachable arguments. | `unreachable_coalesce_argument`, `coalesce_expr_type_unknown`, `parameter_constraint`, and `parameter_type_unresolved` where observed, plus result-column nullability/type absence. |

The Rust tests still carry checks not exposed by this CLI contract: allowed and
rejected input assignments, empty or unrestricted constraint domains, retained
CASE/COALESCE semantic nodes and operands, comparison bindings, and analysis of
a subsequent statement. Keep those assertions. The source test for
`COALESCE(1,$1)` overlaps the assigned `coalesce_parameters.rs` integration
cases. SQLite CASE/COALESCE cases are outside this PostgreSQL-only runner.

One source CASE case, `SELECT CASE WHEN $1 IS NULL THEN 1 ELSE 2 END`, does not
have an equivalent passing public-boundary expectation: the CLI reports
`parameter_type_unresolved` for `$1`, while the lightweight unit context reports
no problem and only checks that the parameter constraint remains unrestricted.
Keep its constraint assertion in Rust; do not count it as superseded by the
fixture. The CLI behavior was observed at `--schema-level builtin` on both
PostgreSQL 17 and 18.

## Additional semantic-only retained groups

These source groups use builtin-named types and synthetic lookup data. Their
assertions cover controlled graph, hint, binding, or candidate-ranking
contracts that the CLI does not expose. Retain them as focused unit coverage;
they are not unresolved migration candidates.

| File/group | Synthetic input | Retained contract |
| --- | --- | --- |
| [`parameter_resolution/indirection_tests.rs`](../../sql-fun-sqlast/src/sem/parameter_resolution/indirection_tests.rs) and its [`test_helpers.rs`](../../sql-fun-sqlast/src/sem/parameter_resolution/indirection_tests/test_helpers.rs) | Controlled parameter-constraint graph relationships for array indexing/slicing, including projections and explicitly injected casts/operators. | Retain graph, edge, and projected-domain assertions; these are internal inference contracts beyond the CLI result schema. |
| [`parse_context/type_hint/tests.rs`](../../sql-fun-sqlast/src/sem/parse_context/type_hint/tests.rs) and its [`test_helpers.rs`](../../sql-fun-sqlast/src/sem/parse_context/type_hint/tests/test_helpers.rs) | Builtin spelling/alias resolution, typed parameters, and `+` constraints use a synthetic type/operator environment. | Retain explicit hint-domain, alias resolution, and synthetic operator binding assertions; these verify parser/type-hint contracts rather than shipped catalog behavior. |
| [`scalar_expr/arith_expr/impl_build_op.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/arith_expr/impl_build_op.rs), [`binary/impl_build_binary_op.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/arith_expr/binary/impl_build_binary_op.rs), [`binary/impl_build_op_any.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/arith_expr/binary/impl_build_op_any.rs), and [`binary/impl_build_op_all.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/arith_expr/binary/impl_build_op_all.rs) | Operator binding and cast candidate tests construct lookup results explicitly. | Retain controlled binding, candidate ranking, and cast-resolution assertions; public `ANY`/`ALL` catalog behavior is covered separately by real-catalog integrations. |

## Retained groups and exclusions

| File/group | Why it stays outside this migration |
| --- | --- |
| [`builtin_bootstrap.rs`](../../sql-fun-sqlast/tests/builtin_bootstrap.rs), [`extracted_artifacts.rs`](../../sql-fun-sqlast/tests/extracted_artifacts.rs), and [`schema_file_loading.rs`](../../sql-fun-sqlast/tests/schema_file_loading.rs) | Their contracts are catalog bootstrap, extracted-artifact compatibility, SQL syntax/version loading, source provenance, and atomic per-file loading. Keep loader regressions at the loader boundary; a consumer fixture would not localize those failures. |
| [`schema_context_chain.rs`](../../sql-fun-sqlast/tests/schema_context_chain.rs) — all groups other than the inherited builtin-overload and migrated cast groups above | They assert ordered immutable schema prefixes, missing-relation/domain diagnostics, descendant isolation, and view application. A loaded builtin root is setup for the schema-layer contract. |
| [`qualified_columns.rs`](../../sql-fun-sqlast/tests/qualified_columns.rs) | Its real-catalog helper is shared setup; assertions concern qualified relation identity, aliases, visibility, and column lineage. The tests do not select builtin overloads or rely on a builtin cast family. |
| [`base_context/builtin_casts.rs`](../../sql-fun-sqlast/src/sem/base_context/builtin_casts.rs), `builtin_fun.rs`, `builtin_ope.rs`, `test_metadata.rs`, and `test_type_categories.rs` | These are direct tests of metadata parsing, builtin catalog construction, and type-category data. Retain them as producer/loader tests; public semantic fixtures cover consumers. |
| [`scalar_expr.rs`](../../sql-fun-sqlast/src/sem/scalar_expr.rs), [`pg_type/type_ref.rs`](../../sql-fun-sqlast/src/sem/pg_type/type_ref.rs), [`type_system/infer.rs`](../../sql-fun-sqlast/src/sem/type_system/infer.rs), and [`type_system/function_overload_collection`](../../sql-fun-sqlast/src/sem/type_system/function_overload_collection.rs) tests | These test generic match, inference, or overload-ranking algorithms against deliberately supplied casts and overloads. `TestParseContext` is the intended focused context for this scope; substituting the large production catalog would weaken control over the algorithm inputs. |
| [`scalar_expr/func_call.rs`](../../sql-fun-sqlast/src/sem/scalar_expr/func_call.rs) and [`data_source/analyze.rs`](../../sql-fun-sqlast/src/sem/data_source/analyze.rs) | They install test-named functions and hand-authored overloads to verify generic binding and table-function behavior, rather than PostgreSQL builtin declarations. |
| [`common_ast_deparse.rs`](../../sql-fun-sqlast/tests/common_ast_deparse.rs), [`common_ast_inspection.rs`](../../sql-fun-sqlast/tests/common_ast_inspection.rs), [`common_ast_mutation.rs`](../../sql-fun-sqlast/tests/common_ast_mutation.rs), `create_{rule,trigger}_ast.rs`, and `drop_{rule,trigger}_ast.rs` | These exercise parser/deparser output, structural inspection, or AST mutation for syntax that can mention builtin types or function names; their assertions do not depend on shipped overload or cast metadata. |
| [`postgres_parse_statements.rs`](../../sql-fun-sqlast/tests/postgres_parse_statements.rs), [`sqlite_parse_statements.rs`](../../sql-fun-sqlast/tests/sqlite_parse_statements.rs) | Dialect parsing and statement-splitting contracts; builtin spelling in source SQL is not catalog resolution. |
| `ast/resolve/query`, `ast/resolve/{insert,update,delete}`, and `ast/document/merge` unit tests using `TestParseContext` | These use builtin type labels and occasional synthetic equality/cast definitions to isolate query/DML resolution, retained source references, and mutation behavior. Their unit assertions are not evidence about PostgreSQL's shipped overload or cast tables. The dedicated integration groups above carry the real-catalog contract. |
| Parser, deparser, serialization, and AST mutation tests that spell `COALESCE`, `GREATEST`/`LEAST`, `ANY`/`ALL`, or builtin types | Their assertions concern syntax retention, formatting, serialization, or AST mutation. A builtin spelling in SQL is not a dependency on builtin catalog semantics. |

## Catalog scope

The accepted migration scope is builtin-only. The CASE/COALESCE fixtures use
`--schema-level builtin` and passed against PostgreSQL 17 and 18. Custom-schema
cases remain in their Rust integration tests; default/project catalog
initialization is tracked separately by #727.
