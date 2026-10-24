# Writable-view SQLAST migration inventory

This inventory maps the SQL-observable assertions from
[`writable_views.rs`](../../sql-fun-sqlast/tests/writable_views.rs) to the
project-schema CLI regression fixtures. The CLI fixtures assert complete
diagnostic sets and ordered result columns. They were checked through
`sql-fun test` with PostgreSQL 17 and 18 catalogs. The corresponding PostgreSQL
17.6 and 18.1 behavior was also checked with small rollback-scoped `EXPLAIN`
probes for automatic view writes, conflict arbiters, generated-column defaults,
duplicate projection assignments, and the invalid rule/trigger declarations.

Each JSON case keeps its original Rust test name in `name`; cases with multiple
observable SQL forms add a suffix. The paired SQL file provides the project
schema for every independent case in that fixture.

## Migrated CLI contracts

| Fixture | Original Rust cases and migrated behavior |
| --- | --- |
| [`schema-views-automatic.json`](schema-views-automatic.json) with [`schema-views-automatic.sql`](schema-views-automatic.sql) | All 10 `automatic_view_query` cases: ordinary and nested INSERT/UPDATE, DEFAULT VALUES, computed-column rejections, RETURNING columns, and ON CONFLICT diagnostics/results. |
| [`schema-views-assignments.json`](schema-views-assignments.json) with [`schema-views-assignments.sql`](schema-views-assignments.sql) | All 5 `view_default_rewrite` cases; all 4 `view_cast_lineage` acceptance/diagnostic cases; all 6 `duplicate_projection_assignments` cases; and the rejected identity/generated assignments from `writable_columns_preserve_effective_constraints`. The Rust test retains its identity, generated, default, and computed-column constraint assertions. |
| [`schema-views-mechanisms.json`](schema-views-mechanisms.json) with [`schema-views-mechanisms.sql`](schema-views-mechanisms.sql) | `independent_write_mechanisms` INSERT and UPDATE outcomes, plus the applicable INSERT RETURNING outcomes. Its Rust test retains internal writable, RETURNING, and ON CONFLICT support flags. |
| [`schema-views-precedence.json`](schema-views-precedence.json) with [`schema-views-precedence.sql`](schema-views-precedence.sql) | All 6 `automatic_rule_precedence` cases: INSERT, INSERT RETURNING, ON CONFLICT, and UPDATE RETURNING diagnostics/results across conditional, additional, instead, and trigger-backed rules. |
| [`schema-views-special.json`](schema-views-special.json) with [`schema-views-special.sql`](schema-views-special.sql) | The write rejections and SELECT result columns from `materialized_views_are_not_write_destinations`; all 4 `from_column_aliases_preserve_source_identity` SQL acceptance/result-column cases. The materialized-view Rust test retains the direct view-write support evaluation. |
| [`schema-views-diagnostics.json`](schema-views-diagnostics.json) with [`schema-views-diagnostics.sql`](schema-views-diagnostics.sql) | All 8 `invalid_view_declarations_are_structured` diagnostic cases and all 14 `semantic_problems_allow_subsequent_statements` diagnostic inputs. These fixture inputs isolate the invalid statement. Their Rust tests retain structured problem-family/code checks and analysis continuation after the error. |

The rule/trigger DDL inputs in the diagnostics fixture produce `query_deparse`
alongside their rule or trigger semantic diagnostic because the CLI cannot
render those statement forms as executable SQL. PostgreSQL 17 and 18 reject the
same invalid statements for the underlying missing relation/rule/trigger. This
extra renderer diagnostic is recorded as the current CLI contract; it does not
indicate a PostgreSQL behavior discrepancy.

Issue [#768](https://github.com/kazuk/sql-fun/issues/768) corrects
`automatic_rule_precedence::instead::conflict` and
`automatic_rule_precedence::conditional_trigger::conflict` in
`schema-views-precedence.json` to expect successful analysis with no columns.
PostgreSQL 17.6 and 18.1 accept both inputs: the first rewrites to nothing,
while the second plans a trigger-backed INSERT with conflict handling.
`view-conflict-rules.json` adds six independent rule-action and UPDATE-rule
boundaries using `view-conflict-rules.sql`; automatic-view exceptions and the
other precedence outcomes remain covered by the migrated fixture.
All added boundaries were checked using EXPLAIN in rolled-back transactions on
both oracles. Ordinary and RETURNING forms remain in the existing fixtures.

## Assertions retained in Rust

| Rust group | Retained contract |
| --- | --- |
| `read_only_shapes` | Internal INSERT/UPDATE view-write applicability for DISTINCT, LIMIT, join, expression, constant, and grouping projections. |
| `writable_columns_preserve_effective_constraints` | Identity, generated, and default constraints carried onto writable view columns, plus omission of a computed column. The diagnostic forms of rejected assignments are in the CLI fixture. |
| `independent_write_mechanisms` | Internal insert/update writability, RETURNING support, and ON CONFLICT support for each trigger/rule mechanism. The SQL-level outcomes are in the CLI fixture. |
| `rule_precedence_support_and_bindings` | Effective INSERT/RETURNING/ON CONFLICT support across all six precedence configurations, bound INSERT target and attached support, retained conflict clause, and typed rejection diagnostics. |
| `view_default_overrides_base_default` | The retained default expression is the view's replacement constant. |
| `sqlite_trigger_oracle`, `sqlite_drop_masks_inherited_trigger`, `sqlite_trigger_namespace_and_column_case_match_engine` | SQLite-specific trigger behavior and the SQLite engine oracle. |
| `function_row_properties`, `declared_function_properties`, `polymorphic_function_properties_use_the_producer_signature` | Internal function row properties and producer-signature use when deciding view writability. |
| `nested_arbiters_retain_the_actual_source`, `view_inference_expands_projection_expressions` | Bound arbiter source relation, column, and index identity through nested views and projected expressions. |
| `later_layers_and_replace_preserve_frozen_parents`, `defaults_and_rule_drops_change_only_the_child`, `select_rule_replacement_refreshes_view_facts_in_the_child`, `replacing_a_view_preserves_named_rule_links_and_refreshes_its_select_rule` | Frozen-parent isolation and view/rule facts across schema-layer changes. |
| `bound_names_do_not_reselect_shadowed_views`, `view_declaration_requires_a_selected_schema` | Search-path target binding and behavior with no selected schema. The CLI fixture harness always selects `public`, so the latter remains at the Rust boundary. |
| `view_write_bindings_and_diagnostics_remain_structured` | INSERT/UPDATE target bindings, view support on bound statements, and typed duplicate-target problem kinds. |
| `incomplete_view_metadata_is_explicit`, `missing_linked_definition_is_incomplete_not_a_dropped_capability` | Missing catalog metadata, missing linked rule/trigger definitions, and their structured view problems. |
| `evaluation_service_reads_definition_options_and_retained_defaults`, `evaluation_excludes_masked_rule_references_and_uses_recreated_definitions`, `evaluation_excludes_cancelled_trigger_links_and_accepts_recreation` | View options/defaults and rule/trigger references across cancellation, masking, and recreation. |
| `rejected_default_changes_leave_the_view_unchanged` | Atomicity of a rejected ALTER VIEW default change in the schema context. |
| `extracted_builtin_rules_reach_production_analysis` | Production `pg_settings` catalog behavior, including its catalog-specific write support and RETURNING limitation. |

`inventory-query.md` predates the project-schema view fixtures and records the
previous builtin-only disposition. This file is the detailed disposition for
`writable_views.rs` after the schema migration.
