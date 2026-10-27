# Schema-dependent regression inventory (#764)

This follow-up supersedes the **builtin-only scope exclusion** in the earlier
inventories. Classification follows individual assertions rather than setup:
SQL diagnostics and public result columns can migrate; catalog layer changes,
AST/binding identity, parameter domains, serialization and API error boundaries
remain Rust contracts. A table marked retained does not imply its SQL setup is
unavailable. Mixed mappings identify which assertions remain.

The migration uses the existing manifest and shared source/installed runner.
`default` loads the actual installation's bundled `schema.sql`; `project` adds
the explicitly selected SQL dump. A project dump is never a replacement builtin
catalog or loaded by selecting `default`. An initial CLI pilot on both versions
confirmed a shipped information_schema domain is available at default, a custom
table/domain/composite is available at project, and the custom table is absent
at default even when `--schema-file` is supplied.

## Detailed migrated and retained mappings

- [DML](inventory-schema-dml.md): named INSERT/UPDATE/DELETE/MERGE cases and
  their individual result, diagnostic and retained binding assertions.
- [Domains and overloads](inventory-schema-domains.md).
- [Schema expressions](inventory-schema-expressions.md).
- [Qualified queries, composites and table functions](inventory-schema-query.md).
- [Writable views](inventory-schema-views.md).

## Schema-layer consumers

| Source test | Disposition and assertions |
| --- | --- |
| `schema_context_chain::full_join_star_publishes_ordered_view_columns` | Migrated to `schema-join.json`, same case name. The original table/view DDL is in `schema-join.sql`; SELECT from the published view checks all three names/order/types and exact nullability. Original width, name order and int8 common type are preserved; the result-only Rust test is removed. Both 17 and 18 pass. |
| `runtime_file_chain_resolves_each_dump_from_its_ordered_prefix` | Retained: the answer type is specifically asserted after each selected ordered prefix, alongside applied-layer availability; one flattened project dump would lose the prefix contract. |
| `inherited_catalog_objects_and_merged_overloads_drive_query_analysis` | Retained: object availability, composite fields, table-row nominal identities, sequence/index keys, view/materialized-view classification and inherited overload merging are checked across distinct frozen layers. Query types verify consumption of that merged environment. A flattened dump does not replace these layer assertions. |
| `sql_array_declarations_survive_schema_layers` | Retained: checks table/view declarations followed by a descendant query across an explicit frozen-layer boundary; all three spellings exercise that same storage/lifetime contract. |
| `missing_from_relation_reports_a_recoverable_problem`, `dependent_view_keeps_missing_relation_problem_after_provider_is_loaded`, `dependent_file_reports_named_missing_domain_base_before_its_provider`, `sql_scalar_subscript_reports_array_required` | Retained: diagnostics belong to schema loading, named structured problems or immutable failed layers, including an invalid view declaration. An invalid preloaded dump produces setup errors, which cannot be expected as query diagnostic codes. |
| `array_item_reads_agree_across_context_layers`, `cloned_prefix_outlives_its_parse_result_and_supports_shared_thread_reads`, `forked_files_keep_definitions_search_paths_provenance_and_problems_local` | Retained: array API parity, shared lifetime/thread reads, descendant isolation, source provenance and search-path state. |
| `tablefunc_crosstab_call_resolves_after_explicit_extension_loading` | Retained: explicit extension loading and composite/function/aggregate catalog APIs. Runner has no configured extension files; this internal loading contract is not silently emulated in a project dump. |
| Remaining alter/drop/index groups in `schema_context_chain.rs` | Retained: inherited identity, schema/search-path lookup, primary-key/index publication, immutable parent/sibling state and recovery after multiple statements. These assert successive operations or direct catalog APIs. |

All rows above refer to
[`schema_context_chain.rs`](../../sql-fun-sqlast/tests/schema_context_chain.rs).

## Inventory sweep and exclusions

The source sweep covered `sql-fun-sqlast/src` and `sql-fun-sqlast/tests` using
`SchemaFileContext`, `TestParseContext`, CREATE/ALTER catalog setup, and direct
lookup assertions, following helper callers. Graphify's graph was absent in
this fresh worktree; source searches were used. Earlier detailed inventories
remain the mapping for already migrated builtin-only scenarios.

| Candidate source/group | Retained/excluded assertion boundary |
| --- | --- |
| `schema_file_loading.rs`, `builtin_bootstrap.rs`, `extracted_artifacts.rs` | Retain producer/bootstrap/artifact compatibility, atomic loading, whole-file diagnostics and provenance. Consumer result cases already mapped in the earlier ARRAY inventory remain fixtures. |
| `sem/schema_file_context/{creation_tests,column_attribute_tests,name_identity_tests}.rs` | Retain creation/alter application, rejected-operation atomicity, collision kind/name, canonical lookup, parent/child isolation, constraint/default/statistics/options persistence and repeated operations. Subsequent SELECT metadata verifies those transitions; independent cases cannot replace the before/after contract. |
| `sem/{create_table,alter_table,create_index,alter_index,column_constraints,constraint,top_level_statement}.rs` tests | Retain catalog APIs, AST conversion, application and constraints, per-object identity or unsupported operation handling. SQL spelling alone does not make these query-result tests. |
| `ast/resolve/{rule,trigger,drop_rule,drop_trigger}` and `sem/view_write/tests.rs` | Retain bound identities, support evaluation, rule/trigger links, masks, dropped/recreated catalog definitions and structured internal failures. View DML consumers are mapped separately above. |
| `ast/resolve/query/tests/{source_namespace,column_scope,whole_rows,call_arguments}.rs` | Retain per-occurrence/table-row bindings, query-level correlation and visibility, failed versus inconclusive lookup state, owned anonymous fields, diagnostic spans, argument positions, serialization and capture before inference. Metadata tied to a selected internal occurrence is part of that binding contract. Database/search-path-sensitive cases are not rewritten for the runner's public-only environment. |
| `ast/resolve/query/tests/{derived_aliases,exists}.rs` and general `query/tests.rs` | Retain binding/AST/clause/parameter-domain tests; standalone derived/literal/EXISTS result assertions without schema setup are outside this schema-dependent follow-up. |
| `ast/resolve/{insert,update,delete}/tests/{views,cursors,versions}*`, `ast/document/merge/tests/{views,assignments,integration}*`, `insert/tests/join_and_key_semantics.rs` | Retain explicitly seeded support/binding/arbiter identities, controlled conversion/domain propagation, cursor state, source-role visibility, version/parser/API parity, AST inspection/mutation and key-origin contracts. Mixed observable assertions in the primary DML groups are mapped separately above. |
| `ast/{ownership,tests,public,ddl}`, common AST integration tests | Retain ownership, parser/deparser/adapter conversion, mutation and serialization contracts. |
| `parameter_resolution.rs`, `case_parameters.rs`, `coalesce_parameters.rs`, `arrays.rs`, `min_max.rs` | Schema result consumers are considered in the expression mapping; remaining assertions concern correlated input/output domains, retained operands/casts/bindings, parser parity or subsequent-statement recovery. |
| `domain_casts.rs`, `type_metadata_regressions.rs`, `qualified_columns.rs`, `writable_views.rs` | Detailed mixed assertion dispositions are in the mappings above; old blanket project-schema exclusions no longer apply. |
| `scalar_any.rs`, `scalar_all.rs`, `variadic_any.rs`, `whole_row_catalog.rs`, `vectors.rs` | Retain selected operator/function overloads, argument/whole-row/catalog-view bindings, cast contexts and serialization. Observable builtin cases already transferred stay in the old fixtures. |
| `sem/parameter_resolution`, `sem/parameter_constraints`, `parse_context/type_hint`, `type_system`, `scalar_expr/arith_expr` | Retain intentionally controlled graph edges/domains, hints, lookup responses, candidate ranking and binding algorithms. Synthetic signatures/conversions are not replaced with real-catalog assumptions. |
| `sem/base_context`, `pg_type/type_ref`, `names/client_type`, `test_helpers` | Retain metadata parsing, catalog construction, exact API/name identity, ownership, type-reference methods and helper contracts. |
| `scalar_expr/{func_call,case_expr,coalesce}`, `data_source/analyze*` | Retain deliberately injected generic signatures, argument/cast/operand graph structure and table-function bindings; standalone builtin-only semantic assertions remain outside the schema follow-up. |
| PostgreSQL/SQLite parse-statement tests and SQLite-only variants throughout | Exclude dialect syntax/splitting and SQLite semantics from this PostgreSQL schema consumer migration. |

No production semantic changes, public API changes, catalog substitution,
new CLI assertion mechanisms or workflow commands are introduced. Non-equivalent
synthetic contexts are retained with evidence, rather than weakened or counted
as superseded. Tests and fixtures remain on maintained execution paths: Rust
workspace tests and the shared source/installed SQLAST regression suite.

## PostgreSQL discrepancies tracked separately

[Bug #765](https://github.com/kazuk/sql-fun/issues/765) corrects implicit
output names for ordinary function calls and scalar subqueries. The migrated
fixtures now assert their function/subquery names, preserving types, nullability
and diagnostics. DML corrections include `delete::resolves_delete::correlated`
and `merge::resolves_merge::returning_subquery`. The added
[`output-names.json`](output-names.json) contract covers aliases, nesting,
parameter inference, derived tables and CTEs. Ordinary-call name corrections
also cover the existing composite, domain-overload and record-expression
fixtures, including diagnostic-bearing calls; their other expectations remain
unchanged. Each of `schema-insert.json`,
`schema-update.json` and `schema-delete.json` also adds one named
`output_names::returning_*` case through its existing schema dump.

[Maintenance #766](https://github.com/kazuk/sql-fun/issues/766) records the
public unresolved-parameter diagnostic versus PostgreSQL PREPARE, explicitly
distinguishing the documented admissible-domain contract from a solver bug.
[Bug #767](https://github.com/kazuk/sql-fun/issues/767) records ARRAY/GREATEST
common-type selection of `int4[]` where PG17/18 preserve an array of domains.
The #767 follow-up corrects those expectations and extends the expression
inventory with order-sensitive ARRAY/GREATEST/LEAST/COALESCE and
parameter-dependent CASE regressions.
[Bug #776](https://github.com/kazuk/sql-fun/issues/776) separately records
parameter-free CASE rejecting domain mixtures through its legacy resolver.

[Bug #768](https://github.com/kazuk/sql-fun/issues/768) addresses INSERT rules
rejecting ON CONFLICT that PostgreSQL accepts. The two corrected precedence
expectations and six additional rule-action boundaries are documented in
[the view inventory](inventory-schema-views.md).
