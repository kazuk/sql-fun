# Diagnostic reference

Use the exact diagnostic code printed by sql-fun to find its meaning, what
analysis established, and possible corrections. Codes are shared with the
analysis API and [`sql-fun test`](../cli/test.md) expectations.

An error prevents complete analysis; a warning may accompany a usable result. A
diagnostic can describe invalid SQL, a missing catalog fact, or an analysis
limitation. It does not by itself establish what a live database will accept.
Each page states the relevant distinction and example prerequisites.

Examples use the selected analysis catalog, without executing SQL. PostgreSQL
and SQLite have different syntax and semantics. The current installed schema
initialization API supports PostgreSQL; pages that describe shared or SQLite
analysis paths do not imply an installed SQLite `analyze` workflow. Some codes
require a custom catalog/context with the metadata stated on the page; those
examples are explicitly illustrative rather than ordinary CLI reproductions.

## Current diagnostic limitations

Some detailed diagnostic renderers remain unimplemented
([#522](https://github.com/kazuk/sql-fun/issues/522)). The CLI still provides the
diagnostic code, severity and explanatory message through its general
presentation path; complete source labels and detailed rendering are not
available for every family. Some rule/trigger messages also group distinct
causes, such as a missing target versus an unsuitable target or function,
without identifying the precise reason. Further semantic-fact work for more
precise diagnostics is tracked in
[#717](https://github.com/kazuk/sql-fun/issues/717). The current message can state
a true general failure without explaining its exact cause.

Several error-reporting paths can still panic before producing a structured
diagnostic ([#523](https://github.com/kazuk/sql-fun/issues/523)). Remaining cases
include missing set-returning function result types, some COALESCE unknown-type
or ambiguity paths, using an ordinary view where a materialized view is
required, unexpected dynamic types, table/CTE
alias-count failures, foreign-key column-type mismatches and SQLite ignored
MATCH-option reporting. Type canonicalization failure also has an unfinished
fatal-error path. This list describes incomplete reporting helpers, not a claim
that every SQL form in those families always reaches them. The diagnostic
catalog below lists available codes; it does not guarantee that every failing
analysis path can deliver one.

Directly downloadable Markdown counterparts of the published HTML manual are
not yet provided by the Pages publication workflow
([#879](https://github.com/kazuk/sql-fun/issues/879)). Markdown files in the source
repository do not establish corresponding `.md` URLs on the documentation site.

## Codes

- [deprecated](deprecated.md)
- [duplicate_lint_target](duplicate_lint_target.md)
- [lint_configuration](lint_configuration.md)
- [drop_column_dependency](drop_column_dependency.md)
- [drop_column_last_column](drop_column_last_column.md)
- [add_column_default](add_column_default.md)
- [add_column_nullability](add_column_nullability.md)
- [add_column_primary_key](add_column_primary_key.md)
- [add_column_sqlite_key](add_column_sqlite_key.md)
- [alter_column_definition_syntax](alter_column_definition_syntax.md)
- [alter_column_duplicate_type](alter_column_duplicate_type.md)
- [alter_column_type_conversion](alter_column_type_conversion.md)
- [alter_column_default_conversion](alter_column_default_conversion.md)
- [alter_column_using_expression](alter_column_using_expression.md)
- [alter_column_invalid_typmod](alter_column_invalid_typmod.md)
- [alter_column_duplicate_option](alter_column_duplicate_option.md)
- [alter_column_identity](alter_column_identity.md)
- [alter_column_invalid_option](alter_column_invalid_option.md)
- [alter_column_missing_column](alter_column_missing_column.md)
- [alter_column_missing_relation](alter_column_missing_relation.md)
- [alter_column_not_stored_generated](alter_column_not_stored_generated.md)
- [alter_column_primary_key](alter_column_primary_key.md)
- [alter_column_reset_value](alter_column_reset_value.md)
- [alter_column_statistics_reduced](alter_column_statistics_reduced.md)
- [alter_column_statistics_too_low](alter_column_statistics_too_low.md)
- [alter_column_unknown_option](alter_column_unknown_option.md)
- [alter_column_version](alter_column_version.md)
- [alter_column_virtual_generated](alter_column_virtual_generated.md)
- [ambiguous_column](ambiguous_column.md)
- [array_empty](array_empty.md)
- [array_incompatible_types](array_incompatible_types.md)
- [array_index_type_mismatch](array_index_type_mismatch.md)
- [array_missing_type_facts](array_missing_type_facts.md)
- [array_subquery_columns](array_subquery_columns.md)
- [binary_operator_overload_ambiguous](binary_operator_overload_ambiguous.md)
- [binary_operator_overload_resolution_failed](binary_operator_overload_resolution_failed.md)
- [call_argument_duplicate_name](call_argument_duplicate_name.md)
- [call_argument_positional_after_named](call_argument_positional_after_named.md)
- [coalesce_expr_type_unknown](coalesce_expr_type_unknown.md)
- [check_boolean_literal](check_boolean_literal.md)
- [check_boolean_type](check_boolean_type.md)
- [check_deferrability](check_deferrability.md)
- [check_duplicate_name](check_duplicate_name.md)
- [check_expression_placement](check_expression_placement.md)
- [check_parameter](check_parameter.md)
- [collate_capability_undefined](collate_capability_undefined.md)
- [collate_operand_type](collate_operand_type.md)
- [collation_name_ambiguous](collation_name_ambiguous.md)
- [collation_not_found](collation_not_found.md)
- [column_already_exists](column_already_exists.md)
- [column_cross_database](column_cross_database.md)
- [column_database_name_required](column_database_name_required.md)
- [column_not_found_in_cte](column_not_found_in_cte.md)
- [column_not_found_in_source](column_not_found_in_source.md)
- [column_not_found_in_sub_query](column_not_found_in_sub_query.md)
- [column_not_found_in_table](column_not_found_in_table.md)
- [column_not_found_in_table_function](column_not_found_in_table_function.md)
- [column_not_found_in_view](column_not_found_in_view.md)
- [column_qualification_invalid](column_qualification_invalid.md)
- [column_source_ambiguous](column_source_ambiguous.md)
- [column_source_not_found](column_source_not_found.md)
- [column_type_not_found](column_type_not_found.md)
- [composite_type_field_not_found](composite_type_field_not_found.md)
- [conflicting_creation](conflicting_creation.md)
- [cross_database_operator_reference](cross_database_operator_reference.md)
- [delete_semantics](delete_semantics.md)
- [distinct_equality_result](distinct_equality_result.md)
- [distinct_row_arity](distinct_row_arity.md)
- [domain_base_type_not_found](domain_base_type_not_found.md)
- [duplicate_alias](duplicate_alias.md)
- [duplicate_def_elem_key](duplicate_def_elem_key.md)
- [explicit_cast_not_defined](explicit_cast_not_defined.md)
- [expression_type_not_known](expression_type_not_known.md)
- [foreign_key_referenced_primary_key_missing](foreign_key_referenced_primary_key_missing.md)
- [function_arg_implicit_cast_not_found](function_arg_implicit_cast_not_found.md)
- [function_not_found](function_not_found.md)
- [call_argument_unknown_name](call_argument_unknown_name.md)
- [call_argument_missing_required](call_argument_missing_required.md)
- [call_argument_assigned_twice](call_argument_assigned_twice.md)
- [function_overload_resolution_failed](function_overload_resolution_failed.md)
- [implicit_cast_not_found](implicit_cast_not_found.md)
- [incompatible_case_values](incompatible_case_values.md)
- [incompatible_engine_version](incompatible_engine_version.md)
- [index_invalid_alteration](index_invalid_alteration.md)
- [index_not_found](index_not_found.md)
- [index_statistics_clamped](index_statistics_clamped.md)
- [inference_candidate_space_unsupported](inference_candidate_space_unsupported.md)
- [insert_semantics](insert_semantics.md)
- [invalid_parameter_hint](invalid_parameter_hint.md)
- [member_access_requires_composite_type](member_access_requires_composite_type.md)
- [membership_comparison_result](membership_comparison_result.md)
- [membership_width](membership_width.md)
- [min_max_empty](min_max_empty.md)
- [min_max_incompatible_types](min_max_incompatible_types.md)
- [min_max_missing_comparison_facts](min_max_missing_comparison_facts.md)
- [min_max_missing_type_facts](min_max_missing_type_facts.md)
- [min_max_not_ordered](min_max_not_ordered.md)
- [operator_name_syntax](operator_name_syntax.md)
- [ownership_argument_type_not_found](ownership_argument_type_not_found.md)
- [ownership_invalid_target_kind](ownership_invalid_target_kind.md)
- [ownership_role_not_found](ownership_role_not_found.md)
- [ownership_target_ambiguous](ownership_target_ambiguous.md)
- [ownership_target_not_found](ownership_target_not_found.md)
- [parameter_constraint](parameter_constraint.md)
- [parameter_type_mismatch_for_table_function](parameter_type_mismatch_for_table_function.md)
- [parameter_type_unresolved](parameter_type_unresolved.md)
- [query_analysis](query_analysis.md)
- [query_deparse](query_deparse.md)
- [query_duplicate_window_definition](query_duplicate_window_definition.md)
- [query_grouping_argument_not_grouped](query_grouping_argument_not_grouped.md)
- [query_invalid_compound_order](query_invalid_compound_order.md)
- [query_invalid_cte_placement](query_invalid_cte_placement.md)
- [query_invalid_grouping_placement](query_invalid_grouping_placement.md)
- [query_invalid_star_projection_placement](query_invalid_star_projection_placement.md)
- [query_parse](query_parse.md)
- [query_position_out_of_range](query_position_out_of_range.md)
- [query_realization](query_realization.md)
- [query_reference_ambiguous](query_reference_ambiguous.md)
- [query_reference_not_found](query_reference_not_found.md)
- [query_scalar_value_required](query_scalar_value_required.md)
- [query_set_column_count](query_set_column_count.md)
- [query_set_column_type](query_set_column_type.md)
- [query_star_projection_requires_composite_type](query_star_projection_requires_composite_type.md)
- [query_too_many_grouping_arguments](query_too_many_grouping_arguments.md)
- [relation_already_exists](relation_already_exists.md)
- [relation_not_found](relation_not_found.md)
- [require_array](require_array.md)
- [rule_invalid_definition](rule_invalid_definition.md)
- [sequence_not_found](sequence_not_found.md)
- [serial_array](serial_array.md)
- [serial_default_conflict](serial_default_conflict.md)
- [serial_generated_conflict](serial_generated_conflict.md)
- [serial_identity_conflict](serial_identity_conflict.md)
- [serial_modifier](serial_modifier.md)
- [serial_nullable](serial_nullable.md)
- [sub_query_not_have_column_for_derived_column_name](sub_query_not_have_column_for_derived_column_name.md)
- [table_function_definition_not_allowed](table_function_definition_not_allowed.md)
- [table_function_definition_placement](table_function_definition_placement.md)
- [table_function_definition_required](table_function_definition_required.md)
- [table_function_duplicate_column](table_function_duplicate_column.md)
- [table_function_invalid_expression](table_function_invalid_expression.md)
- [table_function_invalid_typmod](table_function_invalid_typmod.md)
- [table_function_not_found](table_function_not_found.md)
- [table_function_too_many_aliases](table_function_too_many_aliases.md)
- [table_function_type_not_found](table_function_type_not_found.md)
- [table_not_found](table_not_found.md)
- [tablesample_argument_type](tablesample_argument_type.md)
- [tablesample_arity](tablesample_arity.md)
- [tablesample_expression](tablesample_expression.md)
- [tablesample_invalid_handler](tablesample_invalid_handler.md)
- [tablesample_metadata_unavailable](tablesample_metadata_unavailable.md)
- [tablesample_method_ambiguous](tablesample_method_ambiguous.md)
- [tablesample_method_not_found](tablesample_method_not_found.md)
- [tablesample_relation](tablesample_relation.md)
- [tablesample_repeatable](tablesample_repeatable.md)
- [tablesample_seed_type](tablesample_seed_type.md)
- [trigger_invalid_definition](trigger_invalid_definition.md)
- [truth_test_invalid_literal](truth_test_invalid_literal.md)
- [truth_test_operand_type](truth_test_operand_type.md)
- [unary_operator_overload_ambiguous](unary_operator_overload_ambiguous.md)
- [unary_operator_overload_resolution_failed](unary_operator_overload_resolution_failed.md)
- [undefined_alias](undefined_alias.md)
- [undefined_operator](undefined_operator.md)
- [unreachable_coalesce_argument](unreachable_coalesce_argument.md)
- [unsupported_statement](unsupported_statement.md)
- [update_semantics](update_semantics.md)
- [view_invalid_alter](view_invalid_alter.md)
- [view_invalid_target](view_invalid_target.md)
- [view_missing_write_metadata](view_missing_write_metadata.md)
- [view_not_found](view_not_found.md)
- [view_unknown_default_column](view_unknown_default_column.md)
