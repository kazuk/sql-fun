# Issue #922 emitted diagnostic audit

Baseline: a209bc9b. Static source audit; runtime reproduction remains per-page
work.

## Canonical set

141 emitted codes are documented under `book/diagnostics/`. The table below
records static producer evidence; it is review material, not an input to the
coverage checker. Runtime evidence and limitations are distinguished in the
individual pages and the authoring baseline. The older #924 family inventory
included two additional unproduced classifiers, corrected by this audit.

## Classification and exclusions

`AnalysisProblem::code` delegates to QueryAnalysisProblem/QueryAnalysisStage and
17 nested payload classifiers, plus two Problem::CODE constants for name
resolution. Ownership passes external classification through sem/problem.rs from
sem/alter_owner.rs. No free-form constructor argument by itself establishes a
shipped code.

Excluded `undefined_column` (the core name-resolution variant is never
constructed; the similarly named SQLAST expression reports
`column_not_found_in_source`) and `table_function_collation_not_found`
(declaration collation validation reports the shared collation diagnostics
instead). Also excluded `extension_not_found`, `nullability_unknown`,
`argument_type_mismatch`, `case_when_expr_type_unknown`: source-wide
identifier/call audit finds dispatch, constructors/messages and test-only
consumers, but no production SQLAST/server producer. The extension configuration
error with the same constructor name is a separate error type, not
AnalysisProblem. The fourth constructor is `case_when_type_unknown`, so it must
be audited by variant as well as code name.

Excluded ownership `resolved`: the baseline source witness `ast/ownership.rs:21`
reported target problems only when `target.identity().is_none()`. That analysis
now lives in `sem/alter_owner/ddl.rs`; `OwnershipLookup::identity` in
`sem/alter_owner.rs` returns Some only for `Resolved`. All four failure outcomes
have production lookup branches and flow through the ownership target adapter.
Missing role is separately produced by `Role::resolve_owner`.

Unimplemented TODO-returning helper constructors do not yield AnalysisProblem
codes. Existing readability audit lists these; avoid creating pages for
function_result_set_type_not_found, unknown_type_in_coalesce_expr,
coalesce_expr_type_ambiguous, view_not_matview,
unexpected_dynamic_type, table_not_have_column_for_derived_column_name,
cte_not_have_column_for_derived_column_name, foreign_key_column_type_mismatch,
sqlite_ignores_match_option_in_fk_constraint.

## Severity

All 141 codes have a fixed severity per code. Warning:
`unreachable_coalesce_argument`, `index_statistics_clamped`,
`alter_column_statistics_reduced`. Every other code is Error. Conditional
*family* severity exists for IndexProblem and ColumnAttributeProblem, but their
distinct outcome codes separate Warning from Error; there is no currently
emitted same-code conditional severity. Advice is representable but never
selected by AnalysisProblem::severity. Pilot can use the two statistics cases to
demonstrate a family-dependent severity without inventing a conditional severity
for one code.

## Dialect and runtime boundary

Core classifiers and transport are shared; syntax/semantic producers select
actual applicability. Most PostgreSQL-only structures (ARRAY, GREATEST/LEAST,
PostgreSQL TABLESAMPLE, COLLATE metadata, ownership, detailed ALTER
COLUMN/index) should use PostgreSQL examples; shared query/name/DML/trigger
paths have explicit SQLite branches. Existing installed schema-context
initialization supports PostgreSQL, so do not claim an installed SQLite analyze
workflow merely from SQLAST support. External turso parser wording is not a
stable sql-fun diagnostic contract; errors wrapped by server query pipeline do
have owned stage codes.

## Classifier inventory

| Code | Severity | Family | Classifier |
| --- | --- | --- | --- |
| `alter_column_duplicate_option` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:130` |
| `alter_column_identity` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:123` |
| `alter_column_invalid_option` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:131` |
| `alter_column_missing_column` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:121` |
| `alter_column_missing_relation` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:120` |
| `alter_column_not_stored_generated` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:124` |
| `alter_column_primary_key` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:122` |
| `alter_column_reset_value` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:132` |
| `alter_column_statistics_reduced` | Warning | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:128` |
| `alter_column_statistics_too_low` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:127` |
| `alter_column_unknown_option` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:129` |
| `alter_column_version` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:126` |
| `alter_column_virtual_generated` | Error | alter_column | `sql-fun-core/src/analysis_problem/alter_column.rs:125` |
| `ambiguous_column` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:248` |
| `array_empty` | Error | array | `sql-fun-core/src/analysis_problem/array.rs:40` |
| `array_incompatible_types` | Error | array | `sql-fun-core/src/analysis_problem/array.rs:42` |
| `array_index_type_mismatch` | Error | other | `sql-fun-core/src/analysis_problem.rs:511` |
| `array_missing_type_facts` | Error | array | `sql-fun-core/src/analysis_problem/array.rs:43` |
| `array_subquery_columns` | Error | array | `sql-fun-core/src/analysis_problem/array.rs:41` |
| `binary_operator_overload_ambiguous` | Error | overload_resolution | `sql-fun-core/src/analysis_problem/overload_resolution.rs:60` |
| `binary_operator_overload_resolution_failed` | Error | overload_resolution | `sql-fun-core/src/analysis_problem/overload_resolution.rs:55` |
| `call_argument_duplicate_name` | Error | call_argument | `sql-fun-core/src/analysis_problem/call_argument.rs:32` |
| `call_argument_positional_after_named` | Error | call_argument | `sql-fun-core/src/analysis_problem/call_argument.rs:33` |
| `coalesce_expr_type_unknown` | Error | other | `sql-fun-core/src/analysis_problem.rs:501` |
| `collate_capability_undefined` | Error | collate | `sql-fun-core/src/analysis_problem/collate.rs:36` |
| `collate_operand_type` | Error | collate | `sql-fun-core/src/analysis_problem/collate.rs:37` |
| `collation_name_ambiguous` | Error | collate | `sql-fun-core/src/analysis_problem/collate.rs:38` |
| `collation_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:258` |
| `column_already_exists` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:243` |
| `column_cross_database` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:180` |
| `column_database_name_required` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:179` |
| `column_not_found_in_cte` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:244` |
| `column_not_found_in_source` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:246` |
| `column_not_found_in_sub_query` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:245` |
| `column_not_found_in_table` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:242` |
| `column_not_found_in_table_function` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:241` |
| `column_not_found_in_view` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:247` |
| `column_qualification_invalid` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:181` |
| `column_source_ambiguous` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:183` |
| `column_source_not_found` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:182` |
| `column_type_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:240` |
| `composite_type_field_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:257` |
| `conflicting_creation` | Error | other | `sql-fun-core/src/analysis_problem.rs:472` |
| `delete_semantics` | Error | other | `sql-fun-core/src/analysis_problem.rs:479` |
| `distinct_equality_result` | Error | distinct | `sql-fun-core/src/analysis_problem/distinct.rs:34` |
| `distinct_row_arity` | Error | distinct | `sql-fun-core/src/analysis_problem/distinct.rs:33` |
| `domain_base_type_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:45` |
| `duplicate_alias` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:249` |
| `duplicate_def_elem_key` | Error | other | `sql-fun-core/src/analysis_problem.rs:499` |
| `explicit_cast_not_defined` | Error | other | `sql-fun-core/src/analysis_problem.rs:500` |
| `expression_type_not_known` | Error | other | `sql-fun-core/src/analysis_problem.rs:503` |
| `foreign_key_referenced_primary_key_missing` | Error | other | `sql-fun-core/src/analysis_problem.rs:494` |
| `function_arg_implicit_cast_not_found` | Error | other | `sql-fun-core/src/analysis_problem.rs:513` |
| `function_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:93` |
| `function_overload_resolution_failed` | Error | overload_resolution | `sql-fun-core/src/analysis_problem/overload_resolution.rs:53` |
| `implicit_cast_not_found` | Error | other | `sql-fun-core/src/analysis_problem.rs:514` |
| `incompatible_case_values` | Error | other | `sql-fun-core/src/analysis_problem.rs:509` |
| `incompatible_engine_version` | Error | other | `sql-fun-core/src/analysis_problem.rs:464` |
| `index_invalid_alteration` | Error | index | `sql-fun-core/src/analysis_problem/index.rs:201` |
| `index_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:254` |
| `index_statistics_clamped` | Warning | index | `sql-fun-core/src/analysis_problem/index.rs:199` |
| `inference_candidate_space_unsupported` | Error | other | `sql-fun-core/src/analysis_problem.rs:467` |
| `insert_semantics` | Error | other | `sql-fun-core/src/analysis_problem.rs:477` |
| `invalid_parameter_hint` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1119` |
| `member_access_requires_composite_type` | Error | other | `sql-fun-core/src/analysis_problem.rs:512` |
| `membership_comparison_result` | Error | membership | `sql-fun-core/src/analysis_problem/membership.rs:33` |
| `membership_width` | Error | membership | `sql-fun-core/src/analysis_problem/membership.rs:32` |
| `min_max_empty` | Error | min_max | `sql-fun-core/src/analysis_problem/min_max.rs:56` |
| `min_max_incompatible_types` | Error | min_max | `sql-fun-core/src/analysis_problem/min_max.rs:57` |
| `min_max_missing_comparison_facts` | Error | min_max | `sql-fun-core/src/analysis_problem/min_max.rs:59` |
| `min_max_missing_type_facts` | Error | min_max | `sql-fun-core/src/analysis_problem/min_max.rs:58` |
| `min_max_not_ordered` | Error | min_max | `sql-fun-core/src/analysis_problem/min_max.rs:60` |
| `ownership_argument_type_not_found` | Error | ownership | `sql-fun-sqlast/src/sem/alter_owner.rs:98` |
| `ownership_invalid_target_kind` | Error | ownership | `sql-fun-sqlast/src/sem/alter_owner.rs:97` |
| `ownership_role_not_found` | Error | ownership | `sql-fun-core/src/analysis_problem/ownership.rs:42` |
| `ownership_target_ambiguous` | Error | ownership | `sql-fun-sqlast/src/sem/alter_owner.rs:96` |
| `ownership_target_not_found` | Error | ownership | `sql-fun-sqlast/src/sem/alter_owner.rs:95` |
| `parameter_constraint` | Error | other | `sql-fun-core/src/analysis_problem.rs:490` |
| `parameter_type_mismatch_for_table_function` | Error | other | `sql-fun-core/src/analysis_problem.rs:516` |
| `parameter_type_unresolved` | Error | other | `sql-fun-core/src/analysis_problem.rs:465` |
| `query_analysis` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1121` |
| `query_deparse` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1122` |
| `query_duplicate_window_definition` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:188` |
| `query_grouping_argument_not_grouped` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:184` |
| `query_invalid_compound_order` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:192` |
| `query_invalid_cte_placement` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:187` |
| `query_invalid_grouping_placement` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:185` |
| `query_invalid_star_projection_placement` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:196` |
| `query_parse` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1117` |
| `query_position_out_of_range` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:191` |
| `query_realization` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1118` |
| `query_reference_ambiguous` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:190` |
| `query_reference_not_found` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:189` |
| `query_scalar_value_required` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:178` |
| `query_set_column_count` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:176` |
| `query_set_column_type` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:177` |
| `query_star_projection_requires_composite_type` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:194` |
| `query_too_many_grouping_arguments` | Error | query | `sql-fun-core/src/analysis_problem/query.rs:186` |
| `relation_already_exists` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:252` |
| `relation_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:251` |
| `require_array` | Error | other | `sql-fun-core/src/analysis_problem.rs:505` |
| `rule_invalid_definition` | Error | other | `sql-fun-core/src/analysis_problem.rs:474` |
| `sequence_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:255` |
| `sub_query_not_have_column_for_derived_column_name` | Error | other | `sql-fun-core/src/analysis_problem.rs:507` |
| `table_function_definition_not_allowed` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:78` |
| `table_function_definition_placement` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:77` |
| `table_function_definition_required` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:76` |
| `table_function_duplicate_column` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:79` |
| `table_function_invalid_expression` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:74` |
| `table_function_invalid_typmod` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:75` |
| `table_function_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:238` |
| `table_function_too_many_aliases` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:82` |
| `table_function_type_not_found` | Error | table_function | `sql-fun-core/src/analysis_problem/table_function.rs:80` |
| `table_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:256` |
| `tablesample_argument_type` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:88` |
| `tablesample_arity` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:86` |
| `tablesample_expression` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:90` |
| `tablesample_invalid_handler` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:84` |
| `tablesample_metadata_unavailable` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:82` |
| `tablesample_method_ambiguous` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:83` |
| `tablesample_method_not_found` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:81` |
| `tablesample_relation` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:85` |
| `tablesample_repeatable` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:87` |
| `tablesample_seed_type` | Error | table_sample | `sql-fun-core/src/analysis_problem/table_sample.rs:89` |
| `trigger_invalid_definition` | Error | other | `sql-fun-core/src/analysis_problem.rs:475` |
| `truth_test_invalid_literal` | Error | truth_test | `sql-fun-core/src/analysis_problem/truth_test.rs:29` |
| `truth_test_operand_type` | Error | truth_test | `sql-fun-core/src/analysis_problem/truth_test.rs:28` |
| `unary_operator_overload_ambiguous` | Error | overload_resolution | `sql-fun-core/src/analysis_problem/overload_resolution.rs:61` |
| `unary_operator_overload_resolution_failed` | Error | overload_resolution | `sql-fun-core/src/analysis_problem/overload_resolution.rs:58` |
| `undefined_alias` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:250` |
| `undefined_operator` | Error | other | `sql-fun-core/src/analysis_problem.rs:504` |
| `unreachable_coalesce_argument` | Warning | other | `sql-fun-core/src/analysis_problem.rs:502` |
| `unsupported_statement` | Error | pipeline | `sql-fun-core/src/analysis_problem.rs:1120` |
| `update_semantics` | Error | other | `sql-fun-core/src/analysis_problem.rs:478` |
| `view_invalid_alter` | Error | view | `sql-fun-core/src/analysis_problem/view.rs:54` |
| `view_invalid_target` | Error | view | `sql-fun-core/src/analysis_problem/view.rs:51` |
| `view_missing_write_metadata` | Error | view | `sql-fun-core/src/analysis_problem/view.rs:53` |
| `view_not_found` | Error | name_resolution | `sql-fun-core/src/analysis_problem/name_resolution.rs:253` |
| `view_unknown_default_column` | Error | view | `sql-fun-core/src/analysis_problem/view.rs:52` |

## Coverage check and maintenance

The existing `doc` step invokes `.local/sbin/check-diagnostic-reference` before
mdBook can create missing chapters. It reads the Rust classifier graph, constants
and field/payload delegations, then compares the resulting codes to page files
and both navigation lists. Unknown expressions, incomplete matches, unresolved
or cyclic delegations, and new classifier methods fail rather than disappear.
This is a bounded source scanner, not a general Rust interpreter.

A source-symbol witness guards exclusions and the string-valued ownership
boundary across production workspace modules. It counts variant, constructor
and ownership-type uses per source file, including imports and aliases; it does
not list code strings. Ownership conversion, constructor and success guards are
also checked structurally. New symbol uses require reviewing their actual
emission effect before updating the witness. Do not mechanically regenerate it
to make an unexplained failure pass. Formatting and ordinary unrelated source
edits do not require witness updates.

The #820 import migration preserves the ownership conversion and all producer
branches. The adapter now imports `crate::sem::OwnershipLookup` instead of
repeating that path in its impl and parameter. The structural guard checks the
import binding (direct or grouped) as well as the conversion body. Its witness
count increases from three to four solely because of the new import; no emitter
or diagnostic classification is added.

The #1000 message-fragment formatter adds one `ArgumentTypeMismatch` match in
`analysis_problem.rs`. It only describes an existing payload and does not
construct or emit the excluded diagnostic; its witness count increases by one.

Production entry points come from the root/workspace Cargo manifests, including
explicit binary paths and nested workspace members. The scanner follows ordinary
modules and explicit path attributes, excluding test-only modules. Unsupported
source includes fail; the two existing generated SQLite parser includes are
explicitly allowed because they belong to the external syntax parser rather
than the semantic diagnostic producer boundary. Python 3.11 or later is needed
for standard-library TOML reading; the existing builder provides Python 3.12.
There is no new contributor command or end-user runtime dependency.

One-time acceptance controls covered missing/stale pages, missing navigation,
renamed delegated/constant/ownership codes, unsupported classifier syntax,
incomplete enum dispatch, aliased and Self-based new emitters, root and nested
workspace emitters, dynamic ownership strings, changed ownership conversion,
constructor and success guards, and unsupported source includes. Formatting and
nested comments were positive controls. Temporary fixtures were removed; these
controls are not a retained or recurring regression suite.

## Example evidence and limits

The reviewed pilots reproduce four diagnostics through the worktree-built
CLI/server. The three page-authoring groups reproduced another 37, 33 and 36
code examples respectively against the selected PostgreSQL 18 builtin/project
catalogs. Twelve group-one correction examples and the pilot text input hint
completed without error. PostgreSQL 15/18 oracle queries independently checked
statistics DEFAULT/version behavior, clamping and n_distinct restrictions.
The remaining 29 pages state metadata/API/parser/dialect limitations explicitly;
source reachability is not presented as a successful CLI reproduction.

The derived-table alias-width example reaches its semantic producer but the
current CLI cannot deserialize that payload (a null value is rejected for the
SubQueryNotHaveColumnForDerivedColumnName tuple variant). That existing transport
failure remains outside this documentation change. The diagnostic page records
the user-visible limitation without modifying semantics or transport.

## Operator-name validation (#946)

`sem/name_conversion.rs` classifies malformed operator shapes as
`operator_name_syntax` and parser-accepted database qualification as
`cross_database_operator_reference`. The common-AST operator resolver uses the
same component validator and reports a recoverable invalid binding. The shared
CLI fixture covers the cross-database SQL examples on PostgreSQL 17/18;
malformed internal node shapes are covered at the syntax adapter boundary.
