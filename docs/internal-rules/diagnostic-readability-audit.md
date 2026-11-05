# Diagnostic readability inventory

This is an implementation and review inventory for issue #924. It records the
baseline `26655a0e` paths and the current worktree changes. It is a static source
audit: producer and adapter paths were traced, but this does not claim that
every listed runtime case was reproduced. Test examples are identified as such
and are not treated as runtime reproductions.

## Output path

PostgreSQL input is parsed by `parse_statements/postgres.rs` through `pg_query`;
SQLite input is parsed by `parse_statements/sqlite.rs` through the bundled
Turso parser. Both feed the common SQLAST and `ParseContext` semantic reporting
path. Parse or conversion errors are wrapped in `ParseStatementError`; server
query analysis turns these into `QueryAnalysisProblem` stage diagnostics.
`ParseStatementError::is_syntax` selects `query_parse` for syntax failures and
`unsupported_statement` for non-syntax parser/conversion failures.

Semantic producers call `report_analysis_problem` on the shared context. The
server attaches either a source-backed `DiagnosticPresentation` or the
structured `AnalysisProblem` to `AnalysisProbrem`, while retaining the stable
code, severity, and source-position fields. The JSON Lines API transports these
structured values; it does not render a message. The CLI obtains explanatory
text from `DiagnosticMessage::diagnostic_message`; `render_metadata` supplies
optional labels. Missing metadata therefore does not mean that the CLI has no
message.

The envelope fields in `sql-fun-server-api/src/payloads.rs` and
`sql-fun-client/src/payloads.rs` retain their roles. The current change does
change the serialized contents of some embedded `AnalysisProblem` variants:
expression operands, optional expressions, expression lists, type lists,
argument bindings, and operator candidates now retain structured diagnostic
facts instead of opaque display strings. The new public core fact types are
defined in `sql-fun-core/src/analysis_problem/facts.rs`. Existing
`TypeReference`, `TypeDefinition`, `DefElemKey`, and identity wrappers remain
SQL-facing text representations; this does not serialize or recreate the full
semantic AST. The current API and CLI tests exercise selected fact round trips
and rendering; they are samples, not exhaustive runtime coverage of all codes.

## Representative message changes

These examples compare the baseline formatting path with the current message
contract. The baseline fragments with ellipses are abbreviated Rust `Debug`
trees reconstructed from the source adapters; they are not captured CLI runs.

| Fact | Baseline presentation | Current presentation | Source evidence |
| --- | --- | --- | --- |
| Name | `invalid rule statement: Target(missing)` | `rule target missing does not resolve to a supported relation` | Core rule metadata and the changed CLI family expectation |
| Type | `cannot assign Concrete(ConcreteTypeReference { … }) to Concrete(ConcreteTypeReference { … })` | `cannot assign pg_catalog.text to pg_catalog.int4` | PostgreSQL INSERT/UPDATE assignment producers now use type `Display` |
| Overload | `no matching overload for function public.compute with ArgumentBindingCollection([Some(Concrete(…)), None])` | `no matching overload for function public.compute with (pg_catalog.int4, undetermined type)` | Core overload message and SQLAST argument-fact adapter |
| Expression | Message omitted operand detail (`operator + has no definition for the supplied operands`); the retained operand was Debug text | `operator + has no definition for literal of undetermined type` | `AnalysisProblem::undefined_operator` message test and structured expression adapter |

The examples show four distinct gains: names are presented as SQL names, types
as SQL type spellings, unresolved overload arguments stay explicit, and
expression summaries use known kind/type facts without dumping the semantic
AST. Exact source SQL is not reconstructed for these messages.

## Emitted code families

The following groups are extracted from `AnalysisProblem::code` and the nested
family `code()` dispatches, then checked against production constructors and
reporting paths in SQLAST, core, and server. They are codes with production
emission paths, not a promise that each database dialect can produce every
member. Several families are shared, while syntax and semantic producers remain
dialect-specific.

| Producer family | Emitted codes | Main source areas |
| --- | --- | --- |
| Query pipeline and inputs | `query_parse`, `query_realization`, `invalid_parameter_hint`, `unsupported_statement`, `query_analysis`, `query_deparse`, `parameter_type_unresolved`, `inference_candidate_space_unsupported`, `incompatible_engine_version` | `sql-fun-core/src/analysis_problem.rs`; `sql-fun-server/src/session/query_analysis.rs`; `sql-fun-sqlast/src/parse_statements/` and `context/` |
| Index and column alteration | `index_invalid_alteration`, `index_statistics_clamped`; `alter_column_missing_relation`, `alter_column_missing_column`, `alter_column_primary_key`, `alter_column_identity`, `alter_column_not_stored_generated`, `alter_column_virtual_generated`, `alter_column_version`, `alter_column_statistics_too_low`, `alter_column_statistics_reduced`, `alter_column_unknown_option`, `alter_column_duplicate_option`, `alter_column_invalid_option`, `alter_column_reset_value` | `analysis_problem/{index,alter_column}.rs`; SQLAST `sem/alter_index.rs`, `sem/{alter_index,alter_table}/ddl.rs`, and schema context |
| Creation, ownership, rules, triggers, views | `conflicting_creation`; `ownership_role_not_found`, `ownership_target_not_found`, `ownership_target_ambiguous`, `ownership_invalid_target_kind`, `ownership_argument_type_not_found`; `rule_invalid_definition`; `trigger_invalid_definition`; `view_invalid_target`, `view_unknown_default_column`, `view_missing_write_metadata`, `view_invalid_alter` | `analysis_problem/{creation,ownership,rule,trigger,view}.rs`; SQLAST `sem/{alter_owner,ownership}/ddl.rs`, `sem/{create_rule,create_trigger}/ast.rs`, `sem/{alter_owner,create_rule,create_trigger}.rs`, and `context/` |
| DML and query shape | `insert_semantics`, `update_semantics`, `delete_semantics`; `query_set_column_count`, `query_set_column_type`, `query_scalar_value_required`, `column_database_name_required`, `column_cross_database`, `column_qualification_invalid`, `column_source_not_found`, `column_source_ambiguous`, `query_grouping_argument_not_grouped`, `query_invalid_grouping_placement`, `query_too_many_grouping_arguments`, `query_invalid_cte_placement`, `query_duplicate_window_definition`, `query_reference_not_found`, `query_reference_ambiguous`, `query_position_out_of_range`, `query_invalid_compound_order`, `query_star_projection_requires_composite_type`, `query_invalid_star_projection_placement` | `analysis_problem/{insert,update,delete,query}.rs`; SQLAST `sem/{insert_statement,update_statement}.rs`, `sem/select_statement/ast.rs`, and `sem/query.rs` |
| Table functions and call arguments | `table_function_invalid_expression`, `table_function_invalid_typmod`, `table_function_definition_required`, `table_function_definition_placement`, `table_function_definition_not_allowed`, `table_function_duplicate_column`, `table_function_type_not_found`, `table_function_collation_not_found`, `table_function_too_many_aliases`; `call_argument_duplicate_name`, `call_argument_positional_after_named` | `analysis_problem/{table_function,call_argument}.rs`; SQLAST `sem/data_source/`, `sem/scalar_expr/func_call/implementation/analysis.rs` and `sem/data_source/ast.rs` |
| Expression shape and type checks | `min_max_empty`, `min_max_incompatible_types`, `min_max_missing_type_facts`, `min_max_missing_comparison_facts`, `min_max_not_ordered`; `array_empty`, `array_subquery_columns`, `array_incompatible_types`, `array_missing_type_facts`; `truth_test_operand_type`, `truth_test_invalid_literal`; `collate_capability_undefined`, `collate_operand_type`, `collation_name_ambiguous`; `tablesample_method_not_found`, `tablesample_metadata_unavailable`, `tablesample_method_ambiguous`, `tablesample_invalid_handler`, `tablesample_relation`, `tablesample_arity`, `tablesample_repeatable`, `tablesample_argument_type`, `tablesample_seed_type`, `tablesample_expression`; `distinct_row_arity`, `distinct_equality_result`; `membership_width`, `membership_comparison_result`; `parameter_constraint` | `analysis_problem/{min_max,array,truth_test,collate,table_sample,distinct,membership,parameter_constraint}.rs`; SQLAST `sem/scalar_expr/`, `sem/parameter_constraints.rs`, and `sem/scalar_expr/ast.rs` |
| Name resolution and overload resolution | `domain_base_type_not_found`, `function_not_found`, `table_function_not_found`, `undefined_column`, `column_type_not_found`, `column_not_found_in_table_function`, `column_not_found_in_table`, `column_already_exists`, `column_not_found_in_cte`, `column_not_found_in_sub_query`, `column_not_found_in_source`, `column_not_found_in_view`, `ambiguous_column`, `duplicate_alias`, `undefined_alias`, `relation_not_found`, `relation_already_exists`, `view_not_found`, `index_not_found`, `sequence_not_found`, `table_not_found`, `composite_type_field_not_found`, `collation_not_found`; `function_overload_resolution_failed`, `binary_operator_overload_resolution_failed`, `unary_operator_overload_resolution_failed`, `binary_operator_overload_ambiguous`, `unary_operator_overload_ambiguous` | `analysis_problem/{name_resolution,overload_resolution}.rs`; SQLAST `sem/data_source/ast.rs`, `sem/scalar_expr/`, and `context/schema_file_context/` |
| Other emitted expression diagnostics | `foreign_key_referenced_primary_key_missing`, `duplicate_def_elem_key`, `explicit_cast_not_defined`, `coalesce_expr_type_unknown`, `unreachable_coalesce_argument`, `expression_type_not_known`, `undefined_operator`, `require_array`, `sub_query_not_have_column_for_derived_column_name`, `incompatible_case_values`, `array_index_type_mismatch`, `member_access_requires_composite_type`, `function_arg_implicit_cast_not_found`, `implicit_cast_not_found`, `parameter_type_mismatch_for_table_function` | `analysis_problem.rs` and SQLAST `sem/{create_table,column_constraints}/ddl.rs`, `sem/`, `context/` |

`AnalysisProblem::code` also contains these variants, but the audit found no
production producer for them: `extension_not_found`, `nullability_unknown`,
`argument_type_mismatch`, and `case_when_expr_type_unknown`. Their constructors
or code dispatch alone are not evidence of current emission.

## Severity and dialect scope

`AnalysisProblem::severity` defaults to `Error`. Its current warning cases are
`unreachable_coalesce_argument`, `index_statistics_clamped`, and
`alter_column_statistics_reduced`; the latter two derive severity from their
specific nested problem kind. `DiagnosticSeverity::Advice` is representable in
the transport, but no current `AnalysisProblem::severity` branch emits it.
Severity is independent of message rendering and catalog applicability.

Core code, severity, and message dispatch are shared across SQL dialects. The
actual producer set is dialect-gated: PostgreSQL and SQLite enter through
different parser adapters, and semantic handlers can guard individual
behaviors with the selected `ParseContext` dialect. The inventory above is
therefore a cross-dialect code catalogue, not a claim that every code is
available in both dialects or a per-code dialect matrix. For example, PostgreSQL
conversion and assignment paths coexist with SQLite parser, rename, and trigger
paths. The SQLite external-parser wording boundary follows the maintainer
decision recorded below.

Several older constructor helpers still terminate with `todo!()` rather than
returning an `AnalysisProblem`; their call paths are not emitted diagnostic
codes. These include `function_result_set_type_not_found`,
`unknown_type_in_coalesce_expr`, `coalesce_expr_type_ambiguous`,
`view_not_matview`, `operator_name_syntax`, `unexpected_dynamic_type`,
`table_not_have_column_for_derived_column_name`,
`cte_not_have_column_for_derived_column_name`,
`foreign_key_column_type_mismatch`, and
`sqlite_ignores_match_option_in_fk_constraint`. This audit does not treat those
unimplemented helpers as readability fixes or as evidence that additional
semantic analysis is required.

## Debug formatting audit

The following baseline `Debug` output reached diagnostics through their normal
producer and adapter path; the current worktree fixes the listed cases:

| Baseline producer | Public path | Current worktree state |
| --- | --- | --- |
| PostgreSQL AST conversion errors in `ast/postgres.rs` and `ast/postgres/query.rs` | `AnalysisError` to `ParseStatementError`, then server `unsupported_statement` | Replaced with descriptive conversion messages |
| Assignment AST in `sem/insert_statement.rs`, `sem/update_statement.rs`, and `sem/parameter_resolution.rs` | Semantic problems `insert_semantics`, `update_semantics`, or `parameter_constraint` | Uses readable assignment descriptions |
| Constraint domain in `sem/parameter_constraints.rs` | `ConstraintGraph::conflict` to `parameter_constraint` | Uses readable domain formatting and omits the internal node dump |
| Client type hint parse errors in `sem/client_type/{postgres,sqlite}.rs` and the hint name in `context/type_hint/resolve.rs` | `invalid_parameter_hint` | Reports the hint syntax/value without parser `Debug` output |
| Dialect/kind in `ast/deparse.rs`, `ast/deparse/statement.rs`, and `ast/sqlite_retention.rs` | `query_deparse` or `unsupported_statement` | Uses display names; SQLite retention is an invariant-failure path |
| Dialect and DROP object in `sem/error.rs` and `sem/top_level_statement.rs` | `query_analysis` | Uses descriptive names |
| Rule and trigger problem enums in core metadata | `rule_invalid_definition` and `trigger_invalid_definition` | Uses explanatory messages instead of enum dumps |
| SQL name/debug identities and expression/type facts in ownership, scalar, operator, CASE, and COALESCE diagnostics | Semantic codes in the ownership, overload, and expression families above | Current conversions use SQL-facing names and structured facts |

Two remaining source observations must be kept narrow:

* The bundled SQLite parser's `join_type_from_bytes` formats
  `str::from_utf8(s)` with `{:?}`. Its caller receives raw join-keyword tokens,
  so an unsupported word in a join specification can produce a Rust `Ok(\"...\")`
  wrapper in the parser error, which flows through the boundary-discovery
  `ParseStatementError` to `query_parse`. The adjacent `new_join_type` error
  formats `&str`
  values with `Debug`, which supplies ordinary readable quotes around SQL words.
  This is not a reason to ban all `Debug` formatting. Both call sites are in
  bundled upstream `sql-fun-sqlast/src/sqlite_parser/upstream/parser.rs`; the
  immutable source remains unchanged. The existing generated-parser adaptation
  replaces the first formatter with the actual SQL word for the default common
  parser path. The external parser used by feature-off and legacy APIs still
  retains its upstream error wording. The [maintainer decision][sqlite-decision]
  accepts this as a documented external-parser limitation, not a remaining #924
  defect or #717 deferral. The README and crate API documentation name these
  paths and explain that dependency wording is unstable. Tests preserve failure
  behavior without asserting exact external wording. Legacy depth-limit tests
  use the exported limit and verify acceptance/rejection, without matching the
  dependency's message or fixing a particular numeric limit. The JOIN-message
  regression covers the locally owned adaptation only. Once sql-fun wraps an
  error into its own structured diagnostic or `DiagnosticMessage`, the resulting
  presentation remains subject to #924. Parser ownership and dependencies remain
  unchanged; no dependency Debug text is parsed or rewritten.
* `AstMutationError` in `ast/mutation.rs` has public `Display` messages that use
  `Dialect` and duplicate-name `Debug` formatting. The source audit found no
  non-test call site outside that module and no route from this error type into
  server query diagnostics. It is a public AST-editing API error, not an
  established `AnalysisProblem`/CLI emission path.

Other `Debug` uses found during the audit are not rendered diagnostics: the
parameter-resolution operator name is internal operation state, name-resolution
`Debug` is warning logging, several occurrences are tests, and the upstream
SQLite `eat_assert!` diagnostic is a panic-only assertion. `TypeError` values
also have no direct `AnalysisProblem` conversion or server diagnostic route;
the old implicit-cast debug strings were stored inside an internal error and
were not themselves emitted. Keep those categories distinct when searching for
future leaks.

## Retained facts and issue #717 boundary

The current structured expression facts describe only facts already known at
the producer: SQL construct kind, optional SQL identity, optional type,
independent optional nullability, operand presence, ordered argument/type
collections, and declared operator candidate signature/strictness. SQLAST
adapters construct these in `sem/problem.rs` and nearby expression helpers.
They do not rerun name resolution or type inference, and they do not generate
SQL from the semantic AST.

Two rule/trigger messages currently collapse distinctions that the producers
already know:

* `sem/create_rule/ast.rs` returns the same `RuleProblem::Target` when relation
  lookup finds no target and when lookup resolves a materialized view, which is
  an unsupported rule target. The branches already establish the difference;
  retaining a target-reason discriminator requires no additional analysis.
* `sem/create_trigger/ast.rs` reports the same `TriggerProblem::Function` when
  lookup finds no function, when there is no unique zero-input overload, when
  the return type is missing or not `pg_catalog.trigger`, or when the selected
  function does not preserve view rows. Each failure branch already knows its
  reason; retaining it requires no additional analysis. The human
  message says the function is missing or unusable as a trigger function,
  which is true for all these branches. It does not attribute a specific reason
  when the payload has not retained one.

These are producer-known diagnostic facts, not established #717 semantic
analysis deferrals. No readability case in this audit was shown to require new
semantic analysis. Any future #717 deferral should name the exact absent fact,
its producer and consumer, and the analysis that must compute it; a legacy
payload's loss of already-known information is not sufficient evidence.

## Evidence and review use

Use `AnalysisProblem::code` plus each nested family `code()` match as the code
catalogue; use SQLAST `report_analysis_problem` calls and parser/server adapters
to establish emission routes. Core message tests, CLI rendering tests, and API
serde tests demonstrate selected contracts only. They do not prove that every
dialect, code, or malformed SQL input was executed in a test during this audit.
The standard `check-test-flow --all-features` validation subsequently passed
6,060 workspace tests (63 skipped by the existing configuration) and exercised
the source-tree SQLAST regression runner. For example, the maintained
`arrays-diagnostics` case
`invalid_array_equality_retains_failure::different_elements`, against both
PostgreSQL 17 and 18 builtin catalogs, emitted these CLI explanations for
`SELECT ARRAY[1::oid] = ARRAY['x'::text]`:

```text
binary_operator_overload_resolution_failed
no matching overload for operator = with pg_catalog.oid[] and pg_catalog.text[]

undefined_operator
operator = has no definition for array constructor of type pg_catalog.oid[]
and array constructor of type pg_catalog.text[]
```

This is captured output from the runner, with report decoration removed and
a long line wrapped. Its
fixture assertions verify codes and result semantics; message text is covered
by the focused Rust tests, rather than a new fixture-message contract.

[sqlite-decision]: https://github.com/kazuk/sql-fun/issues/924#issuecomment-6592597357
