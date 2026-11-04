# SQL analysis reference migration record (#952)

This is the review evidence for the syntax-based manual split, not another user
reference or a new recurring check. The intended reader is the maintainer
reviewing coverage and factual corrections. The [issue and approved plan][issue]
define the scope and concurrent authoring strategy.

[issue]: https://github.com/kazuk/sql-fun/issues/952

## Baseline and verification method

Source ranges in this record refer to the former 1,018-line
[`book/sql-analysis.md` at `178c017f`][baseline]. The source has 26 second-level
and two third-level headings. Every one remains reachable at the original URL
and anchor, with a link to its current owner. `book/SUMMARY.md` now groups the
reference by statements, common clauses and expressions; diagnostic navigation
remains independent and complete.

[baseline]: https://github.com/kazuk/sql-fun/blob/178c017f9932f0fcdcaa1493f511dd667c5d222c/book/sql-analysis.md

No-loss means preserving substantive topics and important limitations, not
retaining old wording or inaccurate claims. Each author inspected current
semantic-analysis code, actual diagnostic-reporting paths and maintained tests.
The records below identify destinations, disposition and evidence. Changes to
old support/limitation claims are explicit. Parser representation by itself is
not evidence of completed analysis, and PostgreSQL test evidence is not evidence
of SQLite support.

Statement, clause and expression authors wrote disjoint file sets in one broad
concurrent pass. The coordinator owns the landing page, statement index,
navigation, inbound links, this record, overlap reconciliation and validation.
The shared page contract is Overview; Dialect and engine-specific behavior;
Diagnostics. Pilot review uses SELECT, ALTER TABLE and COLLATE to cover query,
catalog application and shared expressions respectively; their complete final
pages use that same contract. Approval to implement was given in the task
conversation after the plan and maintainer correction were recorded.

## Overview and shared schema policy

| Original range / topic | Destination and disposition | Current evidence and review |
| --- | --- | --- |
| 1–5: audience and installation boundary | `book/sql-analysis.md`: retain and expand syntax navigation; Installation still owns setup. | CLI analyze and server API guides establish diagnostics, parameters/results and explicit selected catalogs as user tasks. |
| 111–121: conflicts across files, namespaces, rejection and continuation | `book/sql-analysis/statements.md#conflicting-schema-definitions`: retain; CREATE leaves link to one shared owner. | `src/context/schema_file_context/creation_tests.rs`: shared relation namespace, child catalogs, same-schema versus different-schema tests, retained original definitions and continued analysis. |
| 122–128: IF NOT EXISTS, OR REPLACE, SQLite case rules and unsupported constraints/indexes | Same shared owner: retain with statement-specific details in leaf pages. | `creation_tests.rs` and `ast/ddl.rs`; SQLite collision tests and statement dispatch distinguish analyzer limits from legal SQLite syntax. |
| 129–131: DROP TABLE does not free a name | Shared policy and `statements/drop-table.md`: retain current metadata-only boundary. | `tests/schema_context_chain.rs` DROP TABLE followed by SELECT, and schema application dispatch. |
| 226–241: SQLite parser depth, UPDATE/DELETE limits, Turso distinction | `book/sql-analysis.md#sqlite-parser-limitations`: retain every caveat and stable anchor. | `src/sqlite_parser/upstream/parser.rs` MAX_EXPR_DEPTH = 100 and grammar; `src/sqlite.rs::test_expression_depth`; SQLite dependency parser is distinct from database build options. |
| 243–259: SQLite support summary and incomplete checks | Landing page and SELECT, clause, name, conditional and parameter pages: split; narrow stale blanket naming caveat to paths actually incomplete. | Actual query/column/CTE resolvers and maintained dialect tests, detailed in author records below. SQLAST feature availability does not establish server initialization availability. |
| 471–481: global unsupported cases and PostgreSQL 18 boundaries | `book/sql-analysis.md#analysis-boundaries` and relevant clause/statement leaves: retain and clarify parsing versus semantics. | `ast/engine_features.rs` compatible-target panic tests for unfinished PostgreSQL 18 features; `ast/ddl.rs` deferrable unique-key `todo!`; current WITH binding and statement dispatch. Ordinary star analysis is distinguished from opt-in executable rewrite. |

Unless another crate is explicitly named, source evidence paths in author
records
are relative to `sql-fun-sqlast/`. `book/` destinations are relative to the
repository. Source line ranges can cover a complete paragraph, bullet or
example;
when one source paragraph mixes responsibilities, its named destinations jointly
account for the claims.

## Statement migration evidence

Source ranges refer to the immutable original `book/sql-analysis.md` at
`178c017f9932f0fcdcaa1493f511dd667c5d222c`. Each row identifies an original
paragraph, list item or SQL example; original wording is an audit input, not a
requirement to copy obsolete prose. Source code and existing tests were
inspected;
this ledger does not claim the tests were run separately by the author.

All destination paths below are relative to `book/sql-analysis/`.

| Source | Original claim/topic (opening text) | Destination | Disposition/current boundary | Evidence group |
| --- | --- | --- | --- | --- |
| L76–L79 | PostgreSQL datasources can opt in to expanding table-dependent wildcard projections in executable SQL. Enable rewrite_options.expand_asterisk = true in the datasource … | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L81–L82 | For example, when the analyzed users table has columns id and name, this query: | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L84–L86 | sql SELECT u.* FROM users AS u  | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L88–L88 | produces executable SQL equivalent to: | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L90–L92 | sql SELECT u.id AS id, u.name AS name FROM users AS u  | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L94–L97 | Expansion uses the supplied schema's column order and preserves output names, including duplicate names. It covers table-dependent projections in nested queries, CTEs, … | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L99–L103 | Only projections depending on a table's column set are expanded. count(*), composite-value expansion such as (record_value).*, and stars over fixed view, … | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L105–L109 | The explicit projection keeps result columns stable across database environments with added columns or dropped unreferenced columns. Every explicitly referenced column … | `statements/select.md` | retain; distinguish opt-in executable rewriting from semantic expansion | S74 |
| L135–L135 | Schema analysis supports these operations on ordinary table columns: | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L137–L139 | - SET NOT NULL and DROP NOT NULL, including their effects on subsequent   query result nullability. Dropping NOT NULL from a primary-key or identity   column is rejected. | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L140–L143 | - SET STATISTICS with integer targets from -1 through 10000. -1 restores   the database default; higher targets are reduced to 10000 with a warning.   The … | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L144–L145 | - SET and RESET for n_distinct and n_distinct_inherited. RESET also accepts   unknown or repeated option names as no-ops, as PostgreSQL does. | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L146–L149 | - DROP EXPRESSION IF EXISTS for stored generated columns. After removal,   INSERT and UPDATE treat the column as an ordinary column. A virtual generated   column … | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L151–L156 | These operations can be combined in one ALTER TABLE statement. Analysis follows PostgreSQL's operation phases: for example, combining SET NOT NULL and DROP NOT NULL leaves … | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L158–L166 | Combining these operations with other ALTER commands, changing a generation expression with SET EXPRESSION, changing storage/compression, generic foreign column options, … | `statements/alter-table.md` | retain/correct; VIRTUAL grammar now recognized and version-gated | S133 |
| L263–L267 | PostgreSQL and SQLite INSERT analysis retains the destination column bindings, VALUES or query source, conflict policy, and RETURNING result names, types, and nullability. … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L269–L276 | Explicit destination columns follow the supplied order. PostgreSQL also permits values for only the first columns when the column list is omitted; SQLite requires values … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L278–L283 | RETURNING resolves destination columns and expands *. PostgreSQL permits alias.* and hides the original table name after an INSERT alias is supplied. SQLite RETURNING … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L285–L301 | DO NOTHING retains and checks conflict targets against catalog constraints and unique indexes, including matching expression keys and identical partial-index predicates. … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L303–L307 | SQLite lookup uses its connection-local, case-insensitive collation namespace. BINARY, NOCASE, and RTRIM are available. API consumers must explicitly declare custom names … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L309–L317 | Modifier-aware arbiter eligibility and general partial-index predicate implication still require additional catalog services and can terminate analysis at their catalog … | `statements/insert.md` | retain; lookup services do not complete modifier-aware inference | S261 |
| L319–L337 | Ordinary view writes are analyzed from the schema declarations you explicitly load. API consumers construct ViewWriteEvaluation::new(&mut context) with the existing … | `statements/create-view.md#writable-views` | split; SQL behavior and explicit SQLAST API subsection | S319 |
| L339–L349 | Automatic views retain direct-column aliases through nested views. DISTINCT, GROUP BY, HAVING, window functions, set-returning projections, joins, set operations, WITH, … | `statements/create-view.md#writable-views` | split; SQL behavior and explicit SQLAST API subsection | S319 |
| L351–L359 | RETURNING is checked independently. An applicable INSTEAD rule requires an unconditional replacement action with RETURNING. Conditional INSTEAD rules block automatic … | `statements/create-view.md#writable-views` | split; SQL behavior and explicit SQLAST API subsection | S319 |
| L361–L371 | Load trigger/rule declarations, replacements, drops, and view defaults after the view declaration, in schema order. Catalog changes in a later schema layer do not mutate a … | `statements/create-view.md#writable-views` | split; SQL behavior and explicit SQLAST API subsection | S319 |
| L375–L381 | PostgreSQL CREATE RULE analysis returns SemAst::rule(), retaining the rule's resolved relation/name identity, event, replacement and INSTEAD options, boolean … | `statements/rules.md` | retain/correct; ARRAY subqueries no longer rejected by pending_rule_semantics | S373 |
| L383–L390 | Rule names belong to their target relation. RuleInfoRead::rule retrieves an exact RuleIdentity; CreateTable::rules, CreateView::rules and ViewLike::rules expose … | `statements/rules.md` | retain/correct; ARRAY subqueries no longer rejected by pending_rule_semantics | S373 |
| L392–L399 | RETURNING requires an unconditional INSTEAD rule with one returning action and matching target column count, order and types. ON SELECT requires an ordinary view, CREATE … | `statements/rules.md` | retain/correct; ARRAY subqueries no longer rejected by pending_rule_semantics | S373 |
| L401–L410 | Actions use the common query/INSERT/UPDATE/DELETE analyzers. Their ordinary limitations apply, including ON CONFLICT DO UPDATE (#474). Within rules, EXISTS (#570), … | `statements/rules.md` | retain/correct; ARRAY subqueries no longer rejected by pending_rule_semantics | S373 |
| L412–L419 | PostgreSQL DROP RULE resolves its ON relation first and retains the exact relation-scoped rule identity, IF EXISTS lookup outcome and dependency choice in … | `statements/rules.md` | retain/correct; ARRAY subqueries no longer rejected by pending_rule_semantics | S373 |
| L423–L431 | PostgreSQL and SQLite CREATE TRIGGER analysis produces a dedicated semantic statement, available through SemAst::trigger(). It retains the resolved target, event … | `statements/triggers.md` | retain; distinct PostgreSQL and SQLite identities/body analysis | S421 |
| L433–L439 | Analysis and catalog application are separate. Applying a valid definition registers it by TriggerIdentity and associates its fully qualified name with the target table … | `statements/triggers.md` | retain; distinct PostgreSQL and SQLite identities/body analysis | S421 |
| L441–L445 | Use a table's triggers() or an ordinary view's triggers() to read the stored names. TriggerReferences::resolve(context, target) resolves them through the same … | `statements/triggers.md` | retain; distinct PostgreSQL and SQLite identities/body analysis | S421 |
| L447–L460 | SQLite accepts unknown UPDATE OF column names. Each event column records whether lookup succeeded and its type when known; an unknown name is not a fabricated column … | `statements/triggers.md` | retain; distinct PostgreSQL and SQLite identities/body analysis | S421 |
| L462–L470 | SQLite DROP TRIGGER uses the schema-scoped trigger namespace, comparing ASCII identifiers without case sensitivity. An unqualified name searches TEMP, main, and the … | `statements/triggers.md` | retain; distinct PostgreSQL and SQLite identities/body analysis | S421 |
| L717–L724 | PostgreSQL and SQLite DELETE analysis retains the qualified target, WITH queries, predicate bindings and RETURNING columns. PostgreSQL USING sources, ONLY/inheritance and … | `statements/delete.md` | retain/correct; PG18 image-alias grammar retained, binding unimplemented | S715 |
| L726–L730 | PostgreSQL RETURNING can reference the target and USING sources. Unqualified *expands only the target; source.* expands the named source. A target alias hides the … | `statements/delete.md` | retain/correct; PG18 image-alias grammar retained, binding unimplemented | S715 |
| L732–L735 | PostgreSQL DELETE CTEs expose RETURNING columns, including explicit CTE column aliases, to later queries. A DELETE CTE without RETURNING can be unreferenced, but cannot be … | `statements/delete.md` | retain/correct; PG18 image-alias grammar retained, binding unimplemented | S715 |
| L737–L743 | The configured engine version governs PostgreSQL RETURNING old/new names: these implicit row references are available starting with PostgreSQL 18. Explicit source … | `statements/delete.md` | retain/correct; PG18 image-alias grammar retained, binding unimplemented | S715 |
| L745–L754 | API consumers can supply DELETE-specific view capabilities through ViewDeleteInfoRead: deletability and RETURNING support are independent of INSERT/UPDATE capabilities. … | `statements/delete.md` | retain/correct; PG18 image-alias grammar retained, binding unimplemented | S715 |
| L758–L762 | PostgreSQL MERGE analysis retains its target, USING source, ON condition and ordered WHEN actions. PostgreSQL 15 and 16 support MATCHED and NOT MATCHED actions. PostgreSQL … | `statements/merge.md` | retain; distinguish custom-provider view facts from production catalog | S756 |
| L764–L770 | MATCHED conditions and UPDATE values can reference both target and source. NOT MATCHED (BY TARGET) can reference only the source; NOT MATCHED BY SOURCE can reference only … | `statements/merge.md` | retain; distinguish custom-provider view facts from production catalog | S756 |
| L772–L784 | RETURNING belongs to the whole MERGE. It can reference target and source columns and merge_action(), including inside RETURNING subqueries. Unqualified * expands … | `statements/merge.md` | retain; distinguish custom-provider view facts from production catalog | S756 |
| L786–L791 | A MERGE CTE exposes only RETURNING columns, with any explicit CTE column aliases. A MERGE CTE without RETURNING may be unreferenced but cannot be read as a relation. … | `statements/merge.md` | retain; distinguish custom-provider view facts from production catalog | S756 |
| L793–L800 | API consumers supply view-target facts through ViewMergeInfoRead. PostgreSQL 17 and later permit automatically updatable views without INSTEAD OF triggers, or views with … | `statements/merge.md` | retain; distinguish custom-provider view facts from production catalog | S756 |
| L804–L809 | Schema analysis supports ALTER INDEX IF EXISTS name SET (...) and RESET (...) for built-in B-tree storage parameters: fillfactor, deduplicate_items, and … | `statements/alter-index.md` | retain | S802 |
| L811–L816 | ALTER INDEX IF EXISTS name ALTER COLUMN n SET STATISTICS value supports expression key columns, addressed by their one-based position in the index. Ordinary key … | `statements/alter-index.md` | retain | S802 |
| L818–L821 | Supported alterations resolve the target against the schema search path and update the index definition used by subsequent schema statements. An invalid statement leaves … | `statements/alter-index.md` | retain | S802 |
| L823–L825 | Other access-method storage parameters, SET TABLESPACE, RENAME, extension dependencies, ALL IN TABLESPACE, and ATTACH PARTITION remain unimplemented. Recognizing … | `statements/alter-index.md` | retain | S802 |
| L892–L897 | ALTER TABLE … RENAME TO … updates the analysis catalog for later statements in explicitly supplied schema files. The new name stays in the resolved table's schema; … | `statements/alter-table.md#rename-table` | retain | S890 |
| L899–L901 | Analysis retains rejected renames and their diagnostics without changing the catalog. PostgreSQL IF EXISTS skips an absent table. SQLite support requires the sqlite … | `statements/alter-table.md#rename-table` | retain | S890 |
| L903–L907 | Foreign keys retain the referenced relation chosen when they were analyzed, including unqualified references; a later search-path change does not change that binding. … | `statements/alter-table.md#rename-table` | retain | S890 |
| L911–L917 | ALTER TABLE … RENAME COLUMN old_name TO new_name changes the column name for later statements in explicitly supplied schema files. The column keeps its position, type, … | `statements/alter-table.md#rename-column` | retain/correct; table-level foreign-key retention now implemented; inheritance/partition gap remains | S909 |
| L919–L931 | An absent source column or occupied destination produces a diagnostic and leaves the catalog unchanged. PostgreSQL rejects renaming a column to itself and supports IF … | `statements/alter-table.md#rename-column` | retain/correct; table-level foreign-key retention now implemented; inheritance/partition gap remains | S909 |
| L933–L938 | Propagation covers dependencies already retained by semantic analysis. Table-level CREATE TABLE foreign-key retention … | `statements/alter-table.md#rename-column` | retain/correct; table-level foreign-key retention now implemented; inheritance/partition gap remains | S909 |
| L942–L947 | ALTER OWNER analysis resolves named owners against the selected datasource's explicit roles metadata(installation.md#configure-known-postgresql-roles). For example, … | `statements/alter-owner.md` | retain | S940 |
| L949–L956 | SQL unquoted names fold to lowercase; quoted names preserve their exact case and escaped quotes. Configured strings are exact catalog names, not SQL expressions. An … | `statements/alter-owner.md` | retain | S940 |
| L958–L959 | This is offline semantic analysis of known role identities. It does not check whether a connected user could transfer ownership or grant database permissions. | `statements/alter-owner.md` | retain | S940 |

### Evidence groups

- **S74**, source L74–L110: implementation `sem/ast_conversion/query.rs`,
  `sem/select_statement/result_column.rs`, `ast/document/projection_walk.rs`;
  maintained tests `sem/ast_conversion/query/tests/source_namespace.rs`,
  `tests/sqlast-regressions/schema-qualified.json`.
- **S133**, source L133–L167: implementation `ast/ddl/column_attributes.rs`,
  `sem/alter_table.rs`, `context/schema_file_context/table_info.rs`; maintained
  tests `ast/ddl/column_attributes.rs::test_applicability`,
  `context/schema_file_context/column_attribute_tests.rs`.
- **S261**, source L261–L318: implementation `sem/ast_conversion/insert.rs`,
  `context/conflict_masters.rs`, `sem/insert_statement/arbiter.rs`; maintained
  tests `sem/ast_conversion/insert/tests.rs`,
  `context/conflict_masters/tests.rs`.
- **S319**, source L319–L372: implementation `sem/view_write.rs`,
  `sem/view_conversion.rs`, `context/schema_file_context/view_info.rs`;
  maintained tests `tests/writable_views.rs`,
  `tests/sqlast-regressions/schema-views-automatic.json`,
  `tests/sqlast-regressions/schema-views-mechanisms.json`.
- **S373**, source L373–L420: implementation `sem/ast_conversion/rule.rs`,
  `sem/ast_conversion/drop_rule.rs`, `context/rule_info.rs`; maintained tests
  `sem/ast_conversion/rule/tests.rs`, `tests/create_rule_ast.rs`,
  `tests/drop_rule_ast.rs`.
- **S421**, source L421–L470: implementation `sem/ast_conversion/trigger.rs`,
  `sem/ast_conversion/drop_trigger.rs`, `context/trigger_info.rs`; maintained
  tests
  `sem/ast_conversion/trigger/tests.rs`, `tests/create_trigger_ast.rs`,
  `tests/drop_trigger_ast.rs`.
- **S715**, source L715–L755: implementation `sem/ast_conversion/delete.rs`,
  `sem/delete_statement/view_support.rs`, `ast/engine_features.rs`; maintained
  tests `sem/ast_conversion/delete/tests.rs`,
  `sem/ast_conversion/delete/tests/versions.rs`,
  `tests/sqlast-regressions/schema-delete.json`.
- **S756**, source L756–L801: implementation `sem/ast_conversion/merge.rs`,
  `sem/merge_statement.rs`, `sem/merge_statement/view_support.rs`; maintained
  tests `sem/ast_conversion/merge/tests.rs`,
  `tests/sqlast-regressions/schema-merge.json`.
- **S802**, source L802–L826: implementation `ast/ddl/alter_index.rs`,
  `sem/alter_index.rs`, `context/schema_file_context.rs`; maintained tests
  `sem/alter_index/tests.rs`.
- **S890**, source L890–L908: implementation `ast/ddl/table_rename.rs`,
  `context/schema_file_context/table_rename.rs`; maintained tests
  `context/schema_file_context/table_rename_tests.rs`.
- **S909**, source L909–L939: implementation `ast/ddl/column_rename.rs`,
  `context/schema_file_context/column_rename.rs`,
  `ast/ddl.rs::Table::resolve_foreign_keys`; maintained tests
  `context/schema_file_context/column_rename_tests.rs`,
  `ast/ddl.rs::test_create_table_foreign_keys`.
- **S940**, source L940–L960: implementation `ast/ownership.rs`,
  `context/schema_file_context/ownership.rs`; maintained tests
  `sem/ownership_conversion.rs`.

### Explicit corrections and their reasons

- Original L143–L148 claimed PostgreSQL 18 VIRTUAL declarations were not parsed.
  `ast/engine_features.rs` now recognizes `GeneratedColumnMode::Virtual` and
  applies the PostgreSQL 18 gate. ALTER TABLE retains the actual DROP EXPRESSION
  inapplicability; remove the obsolete parser assertion.
- Original L403–L410 included ARRAY subqueries among rule-specific pending
  semantics. `SubqueryKind::pending_rule_semantics` in
  `sem/ast_conversion/rule.rs`
  explicitly allows Scalar and Array; `tests::array_query` verifies a CREATE
  RULE containing ARRAY(SELECT...). Keep the still-pending EXISTS,
  quantified/row and multi-expression boundaries and the exact-typmod boundary.
- Original L738–L743 claimed DELETE RETURNING image-alias syntax was rejected by
  the parser. `ast/engine_features.rs` retains PostgreSQL 18 grammar and marks
  renamed-image binding as unimplemented. Keep the binding limitation, correct
  the parser claim, and retain the implicit old/new version rule.
- Original L932–L938 claimed table-level CREATE TABLE foreign-key retention was
  missing (#858). `Table::resolve_foreign_keys` and
`test_create_table_foreign_keys::create_result_and_catalog_retain_composite_and_unnamed_foreign_keys`
  now retain named and unnamed composite foreign keys. The column-rename tests
  inspect table-level FK propagation and creation-time identities. Describe
  current propagation; retain the distinct inheritance/partition limitation.
- Rust/API details from original INSERT view-write discussion have a named
  SQLAST API subsection on CREATE VIEW. The externally observable view-write
  conditions lead that page. No API contract is silently discarded.

### Newly documented statement families

The following pages fill gaps in the old heading inventory; they do not imply
complete syntax support. Statement dispatch alone was not used as proof.

| Page | Implementation evidence and maintained checks | Documented support boundary |
| --- | --- | --- |
| `statements/select.md` | `sem/ast_conversion/query.rs`, `sem/select_statement/set_operation.rs` and adjacent set-operation tests; query clause/source tests | Ordered outputs, projections and set widths/common types; SQLite does not gain PostgreSQL coercions or full aggregate/window legality. |
| `statements/update.md` | `sem/ast_conversion/update.rs` and tests; `tests/sqlast-regressions/schema-update.json` | Assignments, destinations, FROM/predicate/RETURNING, defaults/generated/identity, tuple sources and cursor facts; no runtime updates. |
| `statements/create-table.md` | `ast/ddl.rs` Table/TableAs, `context/schema_file_context/table_info.rs`, inline FK tests; schema loading tests | Columns/constraints/FK bindings and PostgreSQL table-as metadata; SQLite query-backed table path and inheritance remain incomplete. |
| `statements/create-view.md` | `sem/view_conversion.rs`, `context/schema_file_context/view_info.rs`, `sem/view_write.rs`; `tests/writable_views.rs`, schema-views fixtures | SELECT lineage/replacement/catalog and independently evaluated INSERT/UPDATE capabilities, not runtime CHECK OPTION or execution. |
| `statements/alter-view.md` | `ast/ddl.rs::view_default_changes`, `sem/create_view.rs::apply_alter`, view defaults and writable-view tests | Set/drop existing-column defaults; unsupported ALTER forms are not inferred. |
| `statements/create-index.md` | `ast/ddl.rs::Index`, `ast/ddl/alter_index.rs::initialize_index_storage`, schema conflict/index tests | Table-scoped keys/predicate and B-tree options; SQLite and runtime access-method validation are not established. |
| `statements/create-type.md` | `ast.rs::EnumType`, `ast/ddl.rs::CompositeType/CompositeField`, adjacent array field tests; schema type/collation tests | Enum labels and composite declarations; not base/range-type grammar completeness. |
| `statements/create-domain.md` | `ast/ddl.rs::Domain` and `domain_tests`, declaration-collation tests | Base identity/category/declaration collation, not complete domain constraints. |
| `statements/create-function.md` | `ast/ddl.rs::Function/FunctionParameter`, strictness tests, schema routine creation tests | Signature/result/strictness metadata, not body validation; missing set-result reporter is todo!, not an emitted diagnostic. |
| `statements/sequences.md` | `ast.rs::Sequence/AlterSequence`, `context/schema_file_context/sequence_info.rs`, sequence ownership/rename tests | Identity and OWNED BY metadata; numeric options/runtime allocation are not implemented effects. |
| `statements/create-schema.md` | `ast/ddl.rs::Schema`, `context/schema_file_context.rs::apply_creation_group/apply_create_schema`, creation-conflict tests | Staged nested declaration group and explicit authorization, not arbitrary nested utility support. |
| `statements/drop-table.md` | `ast.rs::DropTable`, `context/schema_file_context/table_info.rs::apply_drop_table` and adjacent tests; schema-context chain drop test | Drop metadata does not remove/free table names; IF EXISTS does not currently suppress missing-table findings. |
| `statements/create-extension.md` | `ast.rs::Extension`, `context/schema_file_context.rs::apply_create_extension`, adjacent extension-record test | Records name without loading files/installing extension or applying all syntax options. |
| `statements/set.md` | `ast.rs::VariableSet`, `context/schema_file_context.rs::apply_variable_set`; schema loading/context tests | Single expression recorded; broad RESET/default/session semantics not established. |
| `statements/comment.md` | `ast.rs::Comment`, `ast/deparse/statement_simple.rs`, `context/schema_file_context.rs::apply_comment` | Canonical annotation storage does not resolve target existence or validate permissions. |

All new diagnostic lists were traced to report calls, ResolutionFailure owners,
and existing diagnostic reference pages. Shared expression/clause diagnostics
are
linked to their owner rather than duplicated in each statement inventory.

### Final emission and lookup review

- ALTER TABLE table-rename destination conflicts are `conflicting_creation`,
  through `ast/ddl/table_rename.rs` → `permits_relation_creation` → the shared
  creation reporter. Do not list `relation_already_exists` for that rename path
  merely because another ALTER resolver calls that reporter.
- SQLite `canonical_relation_column` applies case-insensitive normalization to
  table/view columns; derived/CTE output lookups still use exact `ColumnName`
  equality in the relevant paths. SELECT links the detailed name-resolution
  page and retains this narrower limitation instead of claiming all derived
  outputs are case-insensitive. CTE relation-name lookup is a separate
  operation.

## Clause migration and evidence ledger

Source baseline: `178c017f9932f0fcdcaa1493f511dd667c5d222c`, original
`book/sql-analysis.md`.
Destinations below are relative to `book/sql-analysis/`. A retained claim was
checked against the implementation and maintained tests listed in the evidence
keys; this is topic/claim preservation, not preservation of stale wording.

### Evidence keys

- **Q**: `sql-fun-sqlast/src/sem/ast_conversion/query.rs`:
  Query::resolve_query_body,
  Select::resolve_where/resolve_grouping/resolve_having, With::resolve,
  CteBody::resolve, Insert/Update/Delete/Merge::resolve_cte_body,
  Relation/DerivedTable/Join/FromGroup::resolve, Call::resolve_modifiers.
  Tests: `sem/ast_conversion/query/tests.rs`, including
  `infers_parameters_outside_projection`, `binds_aliases_and_ordinals`,
  `preserves_failed_column_resolution`,
  `ctes_are_owned_once_in_declaration_order`,
  `compound_boundary_owns_its_trailing_clauses`,
  `sqlite_retains_clause_parameters`.
- **C**: `sem/ast_conversion/query/clauses.rs`: Expr::resolve_clause (reports
  QueryProblem::position_out_of_range and invalid_compound_order, plus column
  binding failures); Order::checked_name
  (QueryNameResolution::reference_problem),
  Window::resolve_over/resolve/resolve_definitions
  (duplicate_window_definition).
  Tests: query/tests.rs `records_window_resolution`,
  `sqlite_window_inheritance_records_dialect_binding`,
  `sqlite_window_definitions_do_not_bind_projection_aliases`,
  `sqlite_binds_ordering_to_its_output`, `reports_structured_query_problems`.
- **P**: `sem/parameter_resolution.rs`, Collector's predicate, row_count,
  order_operator, frame_offset, window_definition; `sem/query.rs` visitation of
  HAVING/FILTER and `sem/select_statement.rs` visitation of WHERE.
  Tests: query/tests.rs `uses_clause_specific_constraints`,
  `parameter_conflicts_across_clauses_produce_diagnostics` and clause cases
  above.
- **F**: `sem/ast_conversion/query.rs` TableFunctions::resolve/resolve_calls;
  `sem/data_source/function_columns.rs` TableFunctionOutput declaration,
  requirement, placement and alias validation (reports TableFunctionProblem);
  `sem/data_source/resolved.rs` table_function_not_found and
  parameter_type_mismatch_for_table_function emitters. Tests:
  `sem/ast_conversion/query/tests/table_functions.rs` includes
  `diagnoses_invalid_definitions`, `retains_failed_or_ambiguous_binding`,
  `composes_multiple_record_calls_in_written_order`,
  `preserves_ordinality_and_join_nullability`,
  `keeps_selected_overload_separate_from_output_definitions`,
  builtin typmod cases. `sem/with_typmod.rs` interprets builtin precision/scale
  and clamps timestamp/time/interval precision while retaining requested args.
- **D**: `sem/declaration_collation.rs` validation/report: missing/ambiguous,
  operand capability and undefined capability; invoked by F's declaration path.
  Missing type is checked before lookup validation. Query columns are still
  recorded with resolved type/nullability fields after failed validation.
- **R**: `sem/ast_conversion/returning.rs`
  ResultColumnCollection::resolve_for_insert,
  resolve_for_update, resolve_for_delete and Name::require_returning_star.
  These report statement-specific insert/update/delete semantics for invalid
  qualifiers. `sem/scalar_expr/context.rs` owns row-source scopes and
  `sem/data_source/merge_record.rs` merge record projection. Tests:
  `sem/ast_conversion/delete/tests/versions.rs` configured-version, old/new
  stars,
  nested reference, explicit old alias; `sem/ast_conversion/merge/tests.rs` and
  its
  descendants cover MERGE scopes/results. Expression projection checks in
  query.rs / expression resolution emit query-invalid-star-placement and
  query-star-requires-composite diagnostics; table_functions.rs has explicit
  scalar-star and scalar-argument-star cases.
- **E18**: `ast/engine_features.rs` ReturningOption::check_features explicitly
  retains SQL parse/edit/deparse aliases but terminates semantic binding at
  `todo!("RETURNING row-image aliases (#621)")`; tests cover
  insert/update/delete/
  merge and CTE syntax. This supersedes legacy assertion that DELETE aliases
  cannot parse.
- **J**: `sem/data_source/join_expr.rs` and join projection implementation;
  `sem/ast_conversion/query.rs` Join::resolve establishes condition/source
  scope.
  Qualified projection/reference tests preserve merged FULL JOIN values and
  source nullability; shared source namespace code handles duplicates.
- **G**: `sem/scalar_expr/grouping.rs` GroupingExpr::complete/reject_key and
  `sem/ast_conversion` GROUPING resolution: QueryProblem
  grouping_argument_not_grouped,
  invalid_grouping_placement, too_many_grouping_arguments. Tests:
  `sem/ast_conversion/query/grouping_tests.rs`
  keys/aliases/ordinals/ROLLUP/CUBE/SETS,
  correlated placement, limit, nested/aggregate/FILTER/frame errors and max
  args.
- **S**: `sem/data_source/sample.rs` TableSampleSource::resolve,
  SamplingInput::validate and expression-placement validation;
  `sem/sampling_method.rs` SamplingMethodLookup::validate_clause/problem. Actual
  emitters cover metadata unavailable/not found/ambiguous/invalid handler,
  relation, arity, repeatable, argument/seed type and prohibited expressions.
  Maintained tests are adjacent in sample.rs; ordinary provider defaults to
  unavailable metadata. Existing per-code diagnostic pages confirm ordinary
  CLI/provider boundary and are linked individually.
- **CTE**: `sem/ast_conversion/placement.rs`
  CteBody::require_data_modifying_placement
  emits query_invalid_cte_placement; `sem/data_source/cte.rs` emits
  column_not_found_in_cte and missing-derived-column diagnostics.
  Query-body and DML CTE maintained tests cover alias/result and placement.
- **SQ**: `sem/scalar_expr/context.rs`, qualified_source, namespace and column
  lookup paths implement dialect-specific case-insensitive source binding;
  name/expression owner verifies global scope and retains caveats. Clause pages
  avoid the obsolete blanket claim that no SQLite case-insensitive binding
  exists.

### Migrated claims

| Source lines / original topic | Destination | Disposition / evidenced current claim |
| --- | --- | --- |
| 170–172, ROWS FROM list/scalar forms | clauses/from.md#rows-from-and-ordinality | Retain: written output order; SQL values and listed scalar forms. F. |
| 174–178, ROWS FROM example | same | Retain example with no SQL execution claim. F. |
| 180–182, shorter outputs/nullability/ordinality | same | Retain padded nullable results; non-null bigint ordinality except outer extension. F tests. |
| 182–185, aliases/unique references/scalar default/named composite | same | Retain positional aliases, unique lookup, scalar source-name and composite fields. F. |
| 187–189, aggregates/nested SRFs/subquery scope/no execution | same | Retain, actual invalid-expression emitter and own nested query scope. F. |
| 193–194, record output declaration requirement | clauses/from.md#record-returning-functions | Retain, TableFunctionOutput::validate_requirement. F. |
| 196–200, crosstab example | same | Retain with explicit metadata prerequisite. F; no string SQL evaluation. |
| 202–206, catalog/extension objects/string arguments | same | Retain all setup/lookup/no-execution facts. F. |
| 208–212, per-call typed definitions/outer aliases/ordinality | same | Retain placement and positional aliases; actual invalid placement/too-many emitters. F. |
| 214–216, override prohibition/duplicates/missing types/collations | same + Diagnostics | Retain scalar/composite/OUT/TABLE distinction and exact emitted diagnostic links. F,D. |
| 218–220, separate typmods/builtin validation | same | Retain identity-versus-request distinction and char/bit/numeric/time/interval families. F. |
| 221, numeric scale bounds | same | Retain negative scale/scales above precision where valid; WithTypmod::numeric. F. |
| 221–224, precision cap/extension unvalidated/nonexecution | same | Retain requested precision vs interpreted six; extension function not executed. F. |
| 245–247, derived queries/nonrecursive CTE/join nullability | clauses/from.md + with.md | Split with statement SELECT owner, no support assertion inferred from grammar. Q,J. |
| 250–253, recursive/type-affinity/operator-dependent incompleteness | clauses/{from,with,where,having,window}.md | Retain relevant boundaries, replace blanket case-insensitive limitation with implemented-path-specific wording. Q,SQ,P; rest owned by expressions/overview. |
| 254–257, SQLite clause participation/aliases/positions/compound ordering | clauses/{group-by,having,window,order-by,limit-offset}.md | Split and retain; actual aliases/ordinal/compound/window tests. Q,C,P. |
| 257, full grouping/window legality incomplete | clauses/{group-by,having,window,order-by}.md | Retain qualification; added specific implemented GROUPING behavior rather than blanket unsupported claim. C,G. |
| 278–280, INSERT RETURNING aliases/stars/dialect differences | clauses/returning.md | Retain shared RETURNING owner; R. Statement owner links here. |
| 281–283, INSERT CTE RETURNING relation/body dialect | clauses/with.md | Retain; Q,CTE. |
| 349–357, view writability vs RETURNING independence | clauses/returning.md | Shared distinction retained, statement/view owner keeps full rule and trigger capability details; R. |
| 661–664, all clause parameters and RANGE limitations | clauses/{group-by,having,window,order-by,limit-offset}.md | Split: C,P tests; function aggregate FILTER/ordering kept by expression owner via link. |
| 665–668, PostgreSQL aliases/source precedence/HAVING/diagnostics/incomplete legality | clauses/{group-by,having,order-by,window}.md | Retain each precedence and limitation with actual binding tests. C. |
| 717–724, DELETE clause retention/no-execution/no-results | clauses/{where,with,returning}.md | Split shared behavior; target/read-only outcomes remain statement owner. R,Q. |
| 726–729, DELETE RETURNING target/USING/stars/aliases/SQLite | clauses/returning.md | Retain target-only unqualified star, source qualified star, dialect target spelling. R. |
| 732–735, DELETE CTE aliases/readability/top-level/SQLite | clauses/with.md | Retain, Q,CTE; modifying SQLite body boundary explicitly distinct. |
| 737–741, version18 DELETE images/nullability/explicit aliases | clauses/returning.md | Retain, R version tests. |
| 741–743, DELETE RETURNING alias syntax parser rejection | clauses/returning.md | **Correct/replace**: parser now retains aliases; semantic binding todo #621. E18 tests. Important limitation retained accurately. |
| 758–762, MERGE17 CTE/RETURNING and SQLite absence | clauses/{with,returning}.md | Retain shared clause feature gates; MERGE owner keeps actions. Q,R. |
| 772–775, MERGE whole-statement RETURNING/source order | clauses/returning.md | Retain source-first unqualified star, named source projections; action absent source remains MERGE page. R. |
| 776–780, MERGE18 images/nullability/alias precedence/unimplemented rename | clauses/returning.md | Retain version/image possibilities, explicit aliases precedence and rename todo. R,E18. |
| 786–791, MERGE CTE outputs/top-level/shared parameter/no execution | clauses/with.md + returning.md | Retain shared CTE/returning facts and all statement constraints; Q,CTE,P. |
| 829–842, source alias visibility/ambiguous names | clauses/from.md | Link names owner (canonical qualification rules), retain essential source visibility; Q,SQ. |
| 858–863, FULL JOIN merged key order/value/nullability by dialect | clauses/from.md#joins-and-merged-columns | Retain all: PG common type/keys first; SQLite left positions; unmatched values; qualified source null extension. J. |
| 234–237, SQLite DML ORDER BY/LIMIT inherited parser gap | clauses/limit-offset.md | Retain build-option distinction, no setting/preprocessing workaround; overview owns full parser limitation. SQLite parser code/current old source. |
| 98–110, DML executable-SQL wildcard rewrite applicability | clauses/returning.md + from.md links to SELECT | Shared RETURNING points to opt-in rewrite distinct from ordinary result expansion; SELECT owner retains exact option/datasource behavior. |

### Additional implemented coverage

| New reference content | Destination | Evidence and status |
| --- | --- | --- |
| LATERAL versus non-LATERAL source bindings | clauses/from.md | Q DerivedTable::resolve clears same-level FROM for non-LATERAL; supported binding, not complete SQL validator. |
| TABLESAMPLE sources, methods, inputs, REPEATABLE and diagnostics | clauses/from.md#tablesample | S; ordinary CLI incomplete via metadata-unavailable, custom provider verified checks distinguished. No runtime percentage or sampling claim. |
| GROUPING argument/key matching, limit31, placement/ownership | clauses/group-by.md | G; actual emitted code links, not an ordinary function-call support inference. |
| Named windows, inheritance ordering and dialect-specific outcomes | clauses/window.md | C and tests; global legality still incomplete. |
| PostgreSQL predicate bool and row-count bigint input constraints | clauses/{where,having,limit-offset}.md | P plus Q maintained clause parameter tests; no claim to execute constant predicates/counts. |
| RANGE builtin temporal/numeric/interval versus unsupported custom facts | clauses/window.md | P frame_offset and inherited-order tests; parameter-dependent and custom limitations retained. |
| WITH materialization annotations versus execution | clauses/with.md | Q local_definitions retains materialized, no execution planner claim. |
| Statement/query/aggregate/window output alias distinctions | clauses/{order-by,group-by,having,window}.md | C,Q test coverage; no broad alias substitution promised. |

### Validation performed by clause author

`rumdl fmt book/sql-analysis/clauses.md book/sql-analysis/clauses` formats all
ten
owned files. No build/test run (coordinator owns sequential standard checks).
Initial MD057 warnings concern concurrently authored targets only; coordinator
must rerun after integration. Require SELECT's wildcard-projection-expansion
anchor (or adjust shared links). All diagnostic-page target files already exist.
No permanent tests or recurring checks were added.
Final emitter review removed direct implicit_cast_not_found claims for WHERE,
HAVING and LIMIT/OFFSET: parameter Collector::require restricts constraint nodes
without reporting that conversion diagnostic for known non-parameter constants.
The pages describe parameter requirements and incomplete validation separately,
rather than inferring an emission path from the diagnostic definition.

### Post-integration ownership review

RETURNING now owns only shared result naming/star/dialect/input metadata and the
common parser-retained-but-semantic-unimplemented row-image alias caveat. MERGE
retains canonical source-first expansion, merge_action(), possible absent action
rows and PostgreSQL 18 action-dependent images; DELETE retains canonical
old/new image version/nullability. Shared RETURNING links to these owners rather
than duplicating their complete statement-specific explanation. All original
source claims remain accounted for by those canonical pages and ledger rows.

Independent pilot review found: COLLATE correctly separates expression SQLite
resolver (default None, context.rs:218) from conflict-catalog lookup; ALTER
TABLE
correctly replaces obsolete VIRTUAL parser and table-level foreign-key limits.
Reported to coordinator: SELECT must not claim case-insensitive derived/CTE
output
columns; table/view columns canonicalize, CTE relation names case-fold, output
columns/window names still have exact-match paths. Also table-rename conflicts
emit conflicting_creation via permits_relation_creation, not
relation_already_exists.

## Expression migration ledger for #952

Source: `178c017f:book/sql-analysis.md`. Line ranges refer to this immutable
source,
not the replacement landing page. All substantive source paragraphs/examples in
expression-owned sections are listed below; heading anchors are maintained by
the coordinator. Destinations are relative to `book/sql-analysis/expressions/`.
No analyzer behavior changes were made. Legacy wording is not a truth oracle.

### Current implementation and maintained evidence

Evidence keys below identify implementation paths inspected and maintained tests
supporting their contracts. Diagnostic inventories on the destination pages were
traced through actual report calls or shared problem dispatch on these paths;
existing `book/diagnostics/<code>.md` supplies the canonical diagnostic links.

- **R**: `src/sem/ast_conversion/query/tests/rows.rs`,
  `src/sem/scalar_expr/whole_row.rs`, `tests/sqlast-regressions/rows.json` and
  `rows.sql`.
  Constructor field order/names, non-null row metadata, scalar-consumer
  rejection
  and legal SQLite row consumers have dedicated maintained coverage.
- **B**: `src/sem/scalar_expr/arith_expr/between.rs` ordinary/symmetric positive
  and negative comparison composition and local tests; the selected comparison
  paths delegate to existing binary operator resolution and diagnostics. SQL
  NULL examples describe semantics, not constant evaluation by the analyzer.
- **T**: `src/sem/scalar_expr/truth_test.rs` and its tests retain boolean
  operand
  and unknown-literal checks, diagnose through `TruthTestProblem`, preserve
  known non-null truth-test results and participate in parameter constraints.
- **G**: `src/sem/scalar_expr/min_max.rs` common-type/coercion/ordered-type
  facts
  and report calls; `tests/sqlast-regressions/arrays-metadata.json` and the
  min-max retained tests/inventory cover candidate/result/nullability behavior.
- **A**: `src/sem/scalar_expr/array.rs`, `array_subquery.rs` (shape reporting,
  output domains, unknown-to-text query boundary, non-null result),
  `src/sem/scalar_expr/indirect/array_*`, and
  `src/sem/parameter_resolution/indirection_tests.rs`; maintained
  `arrays-{parameters,records,diagnostics,consumers}.json`,
  `array-subqueries.json` and `inventory-array-subqueries.md` establish shipped
  ARRAY(query), constructor typing, arrays of records and pending projections.
  The old #572 pending statement is replaced, not retained as a limitation.
- **P**: `src/context/schema_file_context.rs:827–834` installs resolved hints
  atomically; `src/sem/client_type.rs` and `client_type/{postgres,sqlite}.rs`
  parse dialect spellings; `src/sem/parameter_constraints.rs`,
  `parameter_constraints/call.rs` and `parameter_resolution.rs` retain
  correlated
  admissibility/default witnesses/unsupported-candidate-space reporting;
  `src/sem/query_metadata.rs` reports unresolved final input bindings.
  Maintained
  SQLAST parameter tests plus server `src/session_rpc_tests/query_analysis.rs`
  cover finalization. The page preserves hint API details and the distinction
  between raw SQLAST alternatives, inspection, and the public binding contract.
- **Q**: `src/sem/ast_conversion/query.rs`, `query/clauses.rs`, and
  `query/tests.rs` (including SQLite clause parameter preservation). Clause
  owners document source/output-name precedence, ordering and window boundaries.
- **F**: `src/sem/scalar_expr/func_call.rs:360–510` emits supplied-argument,
  lookup, overload and conversion diagnostics; `src/sem/type_system/
  function_overload_collection.rs` ranks defaults/variadics/polymorphic
  variants.
  Its maintained `tests.rs` covers anyelement/anycompatible/anyarray, arrays,
  vectors, concrete preference, typed parameters, default and variadic calls.
  The blanket legacy unsupported-polymorphic statement is replaced by supported
  families plus remaining candidate-space and range/multirange (#914)
  boundaries.
- **C**: `src/sem/scalar_expr/{case_expr,coalesce,min_max}.rs` plus
  `parameter_resolution.rs`; `coalesce.rs:36–104` preserves conservative
  nullable
  metadata, unreachable warnings, PostgreSQL common types and SQLite arity/no
  PostgreSQL coercion behavior. Maintained `case-common-types.json`,
  `case_semantic.json`, `coalesce_semantic.json` and local SQLite tests support
  the dialect distinction and alternatives retained by CASE/COALESCE.
- **D**: `src/sem/scalar_expr/distinct.rs` and its colocated tests preserve
  bare-null versus typed-null handling, row width and equality-result checks;
  retained custom equality nullability remains explicit. The old #799 domain
  limitation is corrected: `domain-operators.json` includes ordinary domain/base
  equality and IS [NOT] DISTINCT FROM with successful boolean metadata.
  `src/context/schema_file_context/cast_info.rs` and shared parameter call
  matching implement domain reduction. Named-composite/record operator matching
  remains tracked separately by open #804.
- **N**: `src/sem/scalar_expr/context.rs`,
  `column_ref/qualified_column_failure.rs`,
  `src/sem/data_source/qualified.rs` and `resolved.rs`,
  `indirect/member_ref.rs`;
  `tests/qualified_columns.rs` includes `MAIN.T.ID` SQLite table matching, alias
  hiding and explicit database scopes; `query/tests/column_scope.rs` and
  `source_namespace.rs` plus indirection tests cover source and composite
  fields.
  `sem/cte.rs::resolve_name` matches SQLite CTE relation names without ASCII
  case distinctions. `data_source/sub_query.rs` and
  `select_statement/result_column.rs::get_column_with_skip_count` still use
  exact
  derived-query/CTE output-column names; window lookup
  in `scalar_expr/context.rs` compares names exactly. Thus old broad incomplete
  case-insensitivity wording is narrowed rather than globally asserted/fixed.
- **J**: `src/sem/ast_conversion/query/tests/source_namespace.rs` and
  `src/sem/scalar_expr/coalesce.rs::resolve_join_key` cover FULL JOIN merged
  keys
  and dialect column position/type/nullability differences. Canonical owner is
  FROM; names page links to it.
- **L**: `src/sem/scalar_expr/collate.rs:39–101` actual expression lookup/report
  calls and local tests, plus `tests/sqlast-regressions/collate.json`. The only
  expression-resolver implementation is `src/context.rs:218` default `None`.
  Separately, `src/context/conflict_masters.rs:186–221` implements built-in and
  explicitly declared SQLite conflict-collation names. Expression registration
  remains unimplemented in supplied contexts; this is not a contradiction and
  registering a conflict collation does not enable expression COLLATE.
  Declaration `collate_capability_undefined` is explicitly separated from the
  expression diagnostic inventory.
- **M**: `src/sem/scalar_expr/membership.rs` and `membership/tests.rs` analyze
  scalar/row/list/subquery inputs, validate shape and comparison result types,
  and report through `MembershipProblem`; no runtime emptiness/match claim.
- **S**: `src/sem/scalar_expr/arith_expr/{pattern,sqlite_like}.rs`,
  `src/sem/ast_conversion.rs`, and maintained `patterns.json`. SQLite LIKE's
  explicit
  no-return-contract assumption keeps nullability unknown despite operand
  metadata; omitted ESCAPE versus NULL and PostgreSQL catalog operators remain
  distinct. SQLite GLOB/MATCH/REGEXP paths remain unimplemented.

All `src/` paths above are within `sql-fun-sqlast/` unless explicitly identified
as server paths. `tests/sqlast-regressions/` paths are repository-relative;
other `tests/` paths in this expression record are SQLAST crate-relative. No new
tests or recurring checks were added.

### Paragraph/example dispositions

| Original lines | Original claim/topic (excerpt) | Destination | Applicability | Disposition and evidence |
| --- | --- | --- | --- | --- |
| 9–11 | ROW(...) and implicit rows such as (1, true) retain their fields in SQL order, including nested rows. PostgreSQL also permits ROW() and ROW(1). Anonymous fields are named f1, f2, and so on: | `arrays-and-rows.md` | PostgreSQL; SQLite scalar-consumer boundary | Retain; evidence R. |
| 13–15 | sql SELECT (ROW(1, true)).f1, (ROW(1, true)).*  | `arrays-and-rows.md` | PostgreSQL; SQLite scalar-consumer boundary | Retain; evidence R. |
| 17–23 | A row constructor produces one non-null row value even when some or all fields are NULL. For row-valued IS NULL/IS NOT NULL, PostgreSQL tests all fields; a mixed-null row can satisfy neither predicate. … | `arrays-and-rows.md` | PostgreSQL; SQLite scalar-consumer boundary | Retain; evidence R. |
| 25–28 | Qualified ROW(t.*, 42) expands the source fields before the final element. Use ROW(ROW(t.*), 42) to retain those fields as a nested row. Existing compatible named-row casts and record-accepting … | `arrays-and-rows.md` | PostgreSQL; SQLite scalar-consumer boundary | Retain; evidence R. |
| 30–34 | SQLite uses different rules: (value) is scalar grouping and multi-element parentheses are row values. A row value cannot be projected as an ordinary scalar result or supplied to a scalar input such as … | `arrays-and-rows.md` | PostgreSQL; SQLite scalar-consumer boundary | Retain; evidence R. |
| 38–39 | Analysis supports BETWEEN, NOT BETWEEN, and their explicit ASYMMETRIC forms: | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 41–44 | sql SELECT price BETWEEN $1::numeric AND $2::numeric AS within_range FROM products  | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 46–50 | Both endpoints are inclusive. Ordinary BETWEEN preserves the endpoint order; it does not reorder reversed bounds. Each bound must support the corresponding PostgreSQL comparison with the value; compatible … | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 52–57 | NULL follows SQL's three-valued logic: a NULL operand does not always make the whole predicate NULL. For example, 0 BETWEEN 1 AND NULL is false, and 0 NOT BETWEEN 1 AND NULL is true. Result nullability … | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 61–61 | Analysis supports BETWEEN SYMMETRIC and NOT BETWEEN SYMMETRIC: | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 63–66 | sql SELECT price BETWEEN SYMMETRIC $1::numeric AND $2::numeric AS within_range FROM products  | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 68–72 | Either endpoint order is accepted, and both endpoints are inclusive. Operands must support the corresponding PostgreSQL comparisons; compatible mixed types are allowed. NULL follows SQL's three-valued … | `operators-and-predicates.md` | PostgreSQL; SQLite unsupported | Retain; evidence B. |
| 485–490 | IS TRUE, IS FALSE, IS UNKNOWN, and their IS NOT forms produce a non-null boolean, even when the operand is NULL or a nullable column. IS UNKNOWN tests for a NULL boolean operand. These predicates … | `operators-and-predicates.md` | PostgreSQL | Retain; evidence T. |
| 492–496 | Truth predicates constrain parameter inputs to boolean-convertible types. Reusing a parameter in an incompatible expression produces a parameter-constraint diagnostic. As with other parameter analysis, … | `operators-and-predicates.md` | PostgreSQL | Retain; evidence T. |
| 500–504 | GREATEST and LEAST analyze every operand and report a PostgreSQL common result type, implicit coercions, and nullability. NULL operands are ignored; a guaranteed non-null operand makes the result … | `conditional.md` | PostgreSQL | Retain; evidence G. |
| 506–514 | Numbered parameters participate in common-type analysis, including nested expressions, subqueries, and nonrecursive CTEs. Several admissible input types can produce different result types; the analysis … | `conditional.md` | PostgreSQL | Retain; evidence G. |
| 516–519 | Mixed-type selection uses PostgreSQL type categories and the preferred-type flags in builtin cast metadata. If analysis reports missing type or comparison facts, refresh the selected builtin catalog using … | `conditional.md` | PostgreSQL | Retain; evidence G. |
| 523–532 | ARRAY[...] retains its ordered elements and selects a common type using PostgreSQL's type categories and implicit conversions. For example, ARRAY[1,2.5] has type numeric[]; all-unknown strings or … | `arrays-and-rows.md` | PostgreSQL | Correct #572 claim: ARRAY(query) is implemented; evidence A. |
| 534–538 | An empty constructor requires an explicit array cast, such as ARRAY[]::integer[]. The cast also supplies the element type to nested constructors, including nested empty arrays. Nested arrays have the same … | `arrays-and-rows.md` | PostgreSQL | Correct #572 claim: ARRAY(query) is implemented; evidence A. |
| 540–542 | Arrays of anonymous records are supported. PostgreSQL subscripting yields a generic record without accessible field names; use a named composite element type when subsequent SQL needs field references. | `arrays-and-rows.md` | PostgreSQL | Correct #572 claim: ARRAY(query) is implemented; evidence A. |
| 544–546 | Numbered parameters participate in constructor analysis. The result retains admissible input and output types and their relationships; explicit casts constrain the target type. ARRAY(SELECT ...) is … | `arrays-and-rows.md` | PostgreSQL | Correct #572 claim: ARRAY(query) is implemented; evidence A. |
| 550–557 | SQLAST API callers can install explicit SQL input types on a mutable context before calling SemAst::from_ast. Import ParameterTypeHintWrite and call … | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 559–569 | Type hints accept SQL type names for the context's explicitly selected dialect, then resolve them using its catalog and search path. PostgreSQL examples include INTEGER, double precision, character … | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 571–575 | Parameter types describe client-protocol type identities. PostgreSQL parameter OIDs carry no type modifiers, so declarations such as varchar(100), numeric(10,2), timestamp(2) with time zone, and … | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 577–582 | Callers that only need to interpret spelling can use parse_client_type(input, &context) from sql_fun_sqlast::sem. This shared API returns a TypeReference without catalog lookup or search-path … | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 584–586 | Malformed or unresolved names return an error and preserve the complete previous setup. Successful setup replaces all previous hints, including when passed an empty slice or only None entries. | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 588–596 | Create an independent sql_fun_sqlast::context::SchemaFileContext over the shared sql_fun_sqlast::context::FrozenBaseContext catalog for each query. If reusing a mutable context, explicitly install the … | `context/type_hint.rs` rustdoc | PostgreSQL / SQLite API | Retain; identify SQLAST API reader; evidence P. |
| 598–604 | Numbered parameters ($1 in PostgreSQL and ?1 in SQLite) can be used across supported nonrecursive CTEs and the outer statement, including INSERT sources, RETURNING, and conflict predicates. Repeated … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 606–617 | PostgreSQL analysis accounts for implicit casts in operator and ordinary function calls. A parameter or a parameter-dependent result column may have several admissible types. These domains describe … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 619–624 | The public analyze_query API must finalize an application's input binding contract. If SQL inference or a supported explicit input type hint does not establish one input type, it returns … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 626–631 | An unresolved binding does not mean the SQL is intrinsically invalid or that PostgreSQL could not execute it with client-supplied parameter types. PostgreSQL's untyped PREPARE signature is not … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 633–639 | For named application parameters, SELECT ${src:text}::text AS value explicitly supplies the input type and completes with a text binding. Likewise, record_fun(${src:text}::text) resolves the input … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 641–659 | When overloaded operations have no SQL type information for their inputs, PostgreSQL bind-input inference considers seven default candidates: bool, int4, int8, float4, float8, text, and … | `parameters.md` | PostgreSQL; explicitly noted SQLite boundaries | Retain; evidence P. |
| 661–668 | Parameters in supported ORDER BY, GROUP BY, HAVING, DISTINCT ON, LIMIT/OFFSET, aggregate FILTER/ordering, and window expressions participate in analysis, including nested queries. Parameter-dependent RANGE … | `../clauses.md + parameters.md` | PostgreSQL / SQLite clauses | Split; shared clause owners carry details; evidence Q. |
| 670–671 | Concrete PostgreSQL function signatures are supported; unresolved polymorphic signatures remain unsupported. | `function-calls.md` | PostgreSQL | Replace blanket polymorphic unsupported claim with supported families and remaining inference/range boundaries; evidence F. |
| 673–680 | PostgreSQL parameter-dependent CASE expressions constrain searched WHEN conditions to boolean and simple CASE operands through equality overloads. Result branches use implicit common-type conversion, … | `conditional.md` | PostgreSQL / SQLite differences | Retain; split from parameter page; evidence C. |
| 682–689 | PostgreSQL COALESCE uses the same common-type constraints, considering operands in source order. All operands participate, even after a known non-null operand; unreachable operands produce a warning. The … | `conditional.md` | PostgreSQL / SQLite differences | Retain; split from parameter page; evidence C. |
| 691–698 | When every CASE result branch is unresolved, the result domain remains open until input assignments are supplied to ParameterConstraints::allows. This also applies to ARRAY and GREATEST/LEAST. All-unknown … | `conditional.md` | PostgreSQL / SQLite differences | Retain; split from parameter page; evidence C. |
| 702–713 | PostgreSQL IS DISTINCT FROM and IS NOT DISTINCT FROM analyze operand bindings and equality-compatible parameter types. Builtin comparisons return a non-null boolean, including comparisons with NULL. A … | `operators-and-predicates.md` | PostgreSQL / SQLite | Correct obsolete domain/base limitation (#799 resolved), retain named-composite/record limitation under #804; evidence D. |
| 829–834 | PostgreSQL column analysis accepts column, table_or_alias.column, schema.table.column, and database.schema.table.column. References bind only to relations visible in the current query or an … | `names.md` | PostgreSQL | Retain; evidence N. |
| 836–842 | Distinct unaliased relations with the same local name can coexist across schemas: SELECT a.t.id, b.t.id FROM a.t, b.t binds each column to its named relation. The short qualifier t.id (and t.*) is … | `names.md` | PostgreSQL | Retain; evidence N. |
| 844–850 | In SELECT targets, *, alias.*, and schema.table.* expand visible source columns in order. PostgreSQL also expands (composite_expression).*, including named composite values, table rows, and … | `names.md` | PostgreSQL | Retain; evidence N. |
| 852–856 | PostgreSQL member access follows domains over composite types and table row types, including nested domains. For example, (value::address_domain).city returns the declared type of city. If that field is … | `names.md` | PostgreSQL | Retain; evidence N. |
| 858–863 | For PostgreSQL FULL JOIN ... USING and NATURAL FULL JOIN, the merged keys appear first, followed by the remaining left and right columns. Each merged key uses the common type of both inputs and retains … | `../clauses/from.md` | PostgreSQL / SQLite | Link; FROM owns merged join keys; evidence J. |
| 865–866 | To analyze PostgreSQL database-qualified columns, explicitly set the actual database name in the selected datasource metadata: | `names.md + ../../installation.md` | PostgreSQL | Retain required selection/failures; link configuration rather than duplicate TOML; evidence N. |
| 868–874 | toml [package.metadata.database.default] engine = "PostgreSQL" dialect = "postgres" version = "17" database-name = "application"  | `names.md + ../../installation.md` | PostgreSQL | Retain required selection/failures; link configuration rather than duplicate TOML; evidence N. |
| 876–882 | database-name is the exact logical database identifier, without SQL quoting; it must be nonempty and contain no NUL character. It is independent of the metadata datasource name (default in this … | `names.md + ../../installation.md` | PostgreSQL | Retain required selection/failures; link configuration rather than duplicate TOML; evidence N. |
| 884–888 | With the sqlite feature, SQLite uses database.table_or_alias.column, where database is an explicitly referenced SQLite namespace such as main. Identifiers are matched with SQLite's ASCII … | `names.md` | SQLite feature | Clarify: table/view and CTE relation names are case-insensitive; derived/CTE output columns and windows still require matching spelling; evidence N. |
| 963–967 | PostgreSQL COLLATE names are resolved against the loaded collation catalog, using schema qualification or the configured search path. Missing names are diagnosed. An annotation preserves its operand's … | `collate.md` | PostgreSQL | Retain; evidence L. |
| 969–973 | SQLite explicit COLLATE expressions are checked against the analysis metadata. Expression collation registration is not implemented yet, so every explicit name, including BINARY, NOCASE and RTRIM, … | `collate.md` | SQLite | Clarify expression resolver versus separate conflict-collation service; old expression limitation remains current; evidence L. |
| 975–977 | Analysis retains nested annotations and unresolved names. It does not yet derive collations across expressions or check precedence and collation conflicts among function/operator inputs, query boundaries or … | `collate.md` | PostgreSQL / SQLite | Retain; evidence L. |
| 981–985 | IN and NOT IN analyze scalar and row operands with a value list or a subquery. PostgreSQL resolves comparison operators and implicit conversions using the selected catalog. A scalar subquery membership … | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence M. |
| 987–992 | PostgreSQL predicates return boolean; SQLite predicates return integer. NULL inputs can produce NULL results. NOT IN does not turn a NULL comparison into true. Result nullability is conservative: analysis … | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence M. |
| 996–997 | PostgreSQL analysis supports LIKE, NOT LIKE, ILIKE, NOT ILIKE, SIMILAR TO, and NOT SIMILAR TO, including legal ESCAPE clauses: | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence S. |
| 999–1001 | sql SELECT name LIKE $1 ESCAPE '!' AS matches FROM products  | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence S. |
| 1003–1007 | Analysis resolves operand references, operator signatures, conversions and parameter constraints. PostgreSQL results are boolean. LIKE also supports compatible bytea operands; numeric inputs do not gain … | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence S. |
| 1009–1016 | SQLite analysis supports LIKE and NOT LIKE, with an optional ESCAPE expression. Results have integer truth values, and operands follow SQLite's coercion rules rather than PostgreSQL text constraints. … | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence S. |
| 1018–1018 | Analysis does not execute patterns or validate runtime pattern contents. | `operators-and-predicates.md` | PostgreSQL / SQLite | Retain; evidence S. |

### Added coverage

- `types-and-casts.md`: explicit versus implicit/assignment conversion purpose,
  search-path target resolution, row casts and client-hint/declaration-modifier
  distinction. `src/sem/ast_conversion.rs:925–979` emits unresolved-target
  `parameter_constraint` and missing `explicit_cast_not_defined`; shared
  `src/sem/scalar_expr.rs:1094` emits `implicit_cast_not_found`.
- `function-calls.md`: argument order/names/defaults/variadics and polymorphic
  ranking, based on evidence F. Existing code/test evidence replaces legacy
  blanket support omissions; unsupported ranges and unresolved candidate-space
  behavior remain explicitly labeled.
- `arrays-and-rows.md`: shipped ARRAY(query), non-null empty results, output
  width, unresolved parameters/pending expansion and runtime dimension boundary.
- `parameters.md`: `allows` checks correlated assignments, does not install
  hints
  or select bindings; omitted positions versus unknown positions follow the
  existing API contract and tests in `parameter_constraints.rs`.

Validation: `rumdl check book/sql-analysis/expressions.md
book/sql-analysis/expressions` passes all nine owned files. Full repository
flow,
rendered navigation and cross-owner integration remain coordinator-owned.

## Integration review and validation

The coordinator checked that every nonempty substantive source line belongs to
at least one range in the combined records above; headings, blank lines and
formatting delimiters do not replace claim review. Every old heading has a
stable landing-page anchor and a concrete current owner.

SELECT, ALTER TABLE and COLLATE were reviewed as complete pilots against their
actual resolver/reporting paths. Independent review corrected an overclaim
about SQLite derived/CTE output-column case matching and removed an incorrect
ALTER TABLE rename diagnostic attribution: the current shared creation gate
reports `conflicting_creation`. Shared RETURNING rules have one canonical owner;
DELETE/MERGE retain their own action-specific row-image details.

A small PostgreSQL 18 oracle query verified the ROWS FROM/CURRENT_DATE example
and its padded NULL result, and the BETWEEN/NULL truth example. This verifies
SQL behavior only; analyzer support is established by the implementation and
maintained tests above.

The worktree/build filesystem initially had 116 GB available. The planning
baseline was 20 GB; 40 GB was allowed for existing full-check Rust artifacts and
the temporary rendered-manual review tooling. No new contributor or end-user
tool prerequisite, recurring check, permanent regression test, generator or CI
job is introduced by this documentation work.

### Separate pre-existing rendered-link defect

With installed mdBook 0.5.4, the existing `book/cli/analyze.md` link to
`../diagnostics/README.md` renders as `../diagnostics/README.html`, while the
actual diagnostic landing page is `diagnostics/index.html`. Its source link
exists and the diagnostic sidebar correctly uses the index. This is reproducible
by building the unchanged baseline too; it is independent of the SQL reference
split. The existing CLI link is left untouched; this task does not authorize
fixing that separate mdBook/README-link problem. New reference navigation links
to the diagnostic directory entry point or directly to individual code pages.
The remaining reference links and original anchors are validated separately.

### Additional statement review

Independent review of small statement families corrected CREATE INDEX wording:
`ast/ddl.rs::Index::resolve_elements_in` resolves expression elements but
retains
plain key/INCLUDE names directly as `ColumnName`. The page distinguishes that
retention from column-existence validation. Other inspected small-family pages
match their current metadata application and reporting boundaries.

The rendered landing page uses explicit old anchors without promoting all 28
legacy topic labels to headings. This keeps the sidebar centered on the new
statement/clause/expression tree while preserving external fragment URLs.

### Final verification outcome (2026-11-03)

The complete `.local/bin/check-test-flow` succeeded without `--step` on the
integrated change: 5,287 Rust tests passed (63 skipped by the existing profile),
2,725 SQLAST cases in 156 groups completed with no failures, default/all-feature
documentation tests passed, and formatting, lint, package metadata, diagnostic
coverage, Markdown, typos, graph update and skill validation completed. No test
timeouts or introduced slow tests were observed. The flow regenerated the
tracked regression priorities; that generated refresh is committed separately.

The final rendered reference and updated installation/API pages passed 874 local
link/anchor checks across 46 pages. Chromium verified seven representative
pages,
all 28 old anchors and the statement-to-clause-to-expression-to-diagnostic
route.
Screenshots were inspected for the syntax sidebar and page layout. The separate
baseline CLI diagnostic-index link defect recorded above is not a newly added
reference link. Temporary validation scripts, browser tooling and rendered HTML
are not part of the committed product or recurring test suites.

### Open-issue support-boundary additions (2026-11-03)

The maintainer requested explicit documentation for the audit's partial,
missing and internal-only entries. This authorizes documentation, not the
implementation of the reported gaps. The original 61-entry selection contains
an unrelated internal release-workflow item, #297; the maintainer explicitly
excluded it. No release-workflow, contributor-procedure or SemVer-policy change
is included. The remaining 60 audit entries are accounted for below. During
final
verification, #947 was fixed on the updated base; 59 entries therefore retain
current limitations requiring documentation.

Readers and purpose:

- SQL authors use the owning statement/clause/expression page to determine
  whether valid dialect syntax produces trustworthy bindings, result metadata
  and catalog changes. Specific missing operations replace umbrella caveats.
- SQLAST consumers need the distinction between parsed syntax, resolved trees
  and callable metadata accessors. SQL/JSON/XML representation gaps and
  projection-accessor panics are documented at those boundaries.
- CLI users need installation, response-resource and diagnostic delivery
  limitations at the corresponding installation/API/diagnostic pages. These
  three areas do not change SQL validity or introduce new developer obligations.
- Parser-retained SQLite/Turso extensions are distinguished from stock SQLite
  legality. The common statement inventory owns the unsupported entry points;
  existing statement pages link to it instead of copying a second inventory.

Implementation evidence remains the existing common AST, ParseContext,
statement dispatch, declaration resolvers and structured diagnostic adapters.
The additions extend the existing handwritten mdBook pages and diagnostic page
contract; no second documentation system or automated recurring check is added.
Source inspection at the PR head includes `sem/top_level_statement.rs`,
`sem/ast_conversion.rs`, `sem/ast_conversion/query.rs`,
`sem/ast_conversion/document.rs`, `ast/ddl.rs`,
`sem/scalar_expr.rs`, `sem/create_view.rs`, `sem/query.rs` (VALUES),
`sem/data_source/join_expr.rs`, core diagnostic constructors/render metadata,
operator-name conversion, and the authenticated client response reader.

Planning capacity: approximately 93 GB available on the worktree/build
filesystem, above the 20 GB baseline; estimated additional demand is below
20 GB using this task's existing build artifacts. `check-test-flow --list-steps`
was inspected. Validation uses the unchanged standard full check flow, mdBook
and temporary link/coverage checks; existing builder tools suffice. Tests and
coverage generation are not run concurrently. No permanent tests are added.

Corrections to the preliminary audit:

- #947 already had a CLI transport limitation in the diagnostic example, and
  was subsequently fixed by main commit `4c49c93d` during final verification.
  After rebasing, the diagnostic page keeps the base's successful CLI example;
  no obsolete decoding-failure warning is retained. Its mapping below records
  the resolved audit entry, not a current limitation.
- #717's legacy body predates richer structured facts. Current internal evidence
  distinguishes coarse target/function messages from facts requiring new
  semantic analysis. The manual does not claim every old payload gap persists.
- #522 has a readable CLI fallback; missing detailed metadata does not mean
  every diagnostic panics. #523 lists only remaining reporting helper gaps;
  implemented unreachable-COALESCE and dynamic-registration error handling are
  not called unimplemented.
- Implemented declaration collations, FK delete-column lists, partial-index
  predicates, expression-key scope and view column defaults remain explicitly
  supported alongside their issue families' remaining gaps.
- The four issue-status reconciliation entries (#395/#519/#568/#574) are not
  converted into blanket unimplemented claims by this follow-up.

| Audited issue | Reader-facing documentation owner |
| --- | --- |
| [#950](https://github.com/kazuk/sql-fun/issues/950) | `book/installation.md` |
| [#947](https://github.com/kazuk/sql-fun/issues/947) | `book/diagnostics/sub_query_not_have_column_for_derived_column_name.md` — resolved on the updated base; no current limitation |
| [#946](https://github.com/kazuk/sql-fun/issues/946) | `book/sql-analysis/expressions/operators-and-predicates.md` |
| [#891](https://github.com/kazuk/sql-fun/issues/891) | Syntax support completed by #957; remaining analysis limitations are documented at `book/sql-analysis/statements.md` under #955 and #956. |
| [#887](https://github.com/kazuk/sql-fun/issues/887) | `book/sql-analysis/statements/alter-table.md`, `book/sql-analysis/statements/create-table.md` |
| [#879](https://github.com/kazuk/sql-fun/issues/879) | `book/diagnostics/README.md` |
| [#865](https://github.com/kazuk/sql-fun/issues/865) | `book/sql-analysis/expressions.md` |
| [#864](https://github.com/kazuk/sql-fun/issues/864) | `book/sql-analysis/expressions.md` |
| [#797](https://github.com/kazuk/sql-fun/issues/797) | `book/sql-analysis/expressions/operators-and-predicates.md` |
| [#792](https://github.com/kazuk/sql-fun/issues/792) | `book/sql-analysis/expressions/operators-and-predicates.md` |
| [#781](https://github.com/kazuk/sql-fun/issues/781) | `book/cli/api.md` |
| [#717](https://github.com/kazuk/sql-fun/issues/717) | `book/diagnostics/README.md` |
| [#657](https://github.com/kazuk/sql-fun/issues/657) | `book/sql-analysis/statements/create-table.md` |
| [#640](https://github.com/kazuk/sql-fun/issues/640) | `book/sql-analysis/statements/create-function.md` |
| [#575](https://github.com/kazuk/sql-fun/issues/575) | `book/sql-analysis/clauses/having.md`, `book/sql-analysis/clauses/where.md` |
| [#571](https://github.com/kazuk/sql-fun/issues/571) | `book/sql-analysis/expressions/operators-and-predicates.md`, `book/sql-analysis/statements/rules.md` |
| [#566](https://github.com/kazuk/sql-fun/issues/566) | `book/sql-analysis/statements/create-function.md` |
| [#565](https://github.com/kazuk/sql-fun/issues/565) | `book/sql-analysis/statements/insert.md`, `book/sql-analysis/statements/select.md` |
| [#564](https://github.com/kazuk/sql-fun/issues/564) | `book/sql-analysis/statements/create-table.md` |
| [#563](https://github.com/kazuk/sql-fun/issues/563) | `book/sql-analysis/statements/create-table.md` |
| [#561](https://github.com/kazuk/sql-fun/issues/561) | `book/sql-analysis/statements.md` |
| [#560](https://github.com/kazuk/sql-fun/issues/560) | `book/sql-analysis/statements.md` |
| [#559](https://github.com/kazuk/sql-fun/issues/559) | `book/sql-analysis/statements.md` |
| [#558](https://github.com/kazuk/sql-fun/issues/558) | `book/sql-analysis/statements.md` |
| [#557](https://github.com/kazuk/sql-fun/issues/557) | `book/sql-analysis/statements.md` |
| [#556](https://github.com/kazuk/sql-fun/issues/556) | `book/sql-analysis/statements.md` |
| [#554](https://github.com/kazuk/sql-fun/issues/554) | `book/sql-analysis/statements.md` |
| [#553](https://github.com/kazuk/sql-fun/issues/553) | `book/sql-analysis/statements.md` |
| [#552](https://github.com/kazuk/sql-fun/issues/552) | `book/sql-analysis/statements.md` |
| [#551](https://github.com/kazuk/sql-fun/issues/551) | `book/sql-analysis/statements.md` |
| [#550](https://github.com/kazuk/sql-fun/issues/550) | `book/sql-analysis/statements.md` |
| [#549](https://github.com/kazuk/sql-fun/issues/549) | `book/sql-analysis/statements.md` |
| [#547](https://github.com/kazuk/sql-fun/issues/547) | `book/sql-analysis/statements/create-index.md` |
| [#546](https://github.com/kazuk/sql-fun/issues/546) | `book/sql-analysis/statements/alter-table.md`, `book/sql-analysis/statements/create-table.md` |
| [#544](https://github.com/kazuk/sql-fun/issues/544) | `book/sql-analysis/statements/alter-view.md` |
| [#543](https://github.com/kazuk/sql-fun/issues/543) | `book/sql-analysis/statements/create-type.md` |
| [#541](https://github.com/kazuk/sql-fun/issues/541) | `book/sql-analysis/statements/alter-table.md` |
| [#540](https://github.com/kazuk/sql-fun/issues/540) | `book/sql-analysis/statements/alter-table.md` |
| [#539](https://github.com/kazuk/sql-fun/issues/539) | `book/sql-analysis/statements/alter-table.md` |
| [#538](https://github.com/kazuk/sql-fun/issues/538) | `book/sql-analysis/statements/alter-table.md` |
| [#537](https://github.com/kazuk/sql-fun/issues/537) | `book/sql-analysis/statements/alter-table.md` |
| [#536](https://github.com/kazuk/sql-fun/issues/536) | `book/sql-analysis/statements/alter-table.md` |
| [#535](https://github.com/kazuk/sql-fun/issues/535) | `book/sql-analysis/statements/alter-table.md` |
| [#534](https://github.com/kazuk/sql-fun/issues/534) | `book/sql-analysis/statements/alter-table.md` |
| [#533](https://github.com/kazuk/sql-fun/issues/533) | `book/sql-analysis/statements/alter-table.md` |
| [#531](https://github.com/kazuk/sql-fun/issues/531) | `book/sql-analysis/statements/alter-table.md` |
| [#526](https://github.com/kazuk/sql-fun/issues/526) | `book/sql-analysis/statements/create-table.md` |
| [#525](https://github.com/kazuk/sql-fun/issues/525) | `book/sql-analysis/statements/create-table.md` |
| [#523](https://github.com/kazuk/sql-fun/issues/523) | `book/diagnostics/README.md` |
| [#522](https://github.com/kazuk/sql-fun/issues/522) | `book/diagnostics/README.md` |
| [#521](https://github.com/kazuk/sql-fun/issues/521) | `book/sql-analysis/statements/select.md` |
| [#506](https://github.com/kazuk/sql-fun/issues/506) | `book/sql-analysis/clauses/from.md` |
| [#505](https://github.com/kazuk/sql-fun/issues/505) | `book/sql-analysis/expressions/function-calls.md` |
| [#498](https://github.com/kazuk/sql-fun/issues/498) | `book/sql-analysis/expressions.md` |
| [#497](https://github.com/kazuk/sql-fun/issues/497) | `book/sql-analysis/expressions.md` |
| [#496](https://github.com/kazuk/sql-fun/issues/496) | `book/sql-analysis/expressions/operators-and-predicates.md` |
| [#491](https://github.com/kazuk/sql-fun/issues/491) | `book/sql-analysis/clauses/from.md` |
| [#490](https://github.com/kazuk/sql-fun/issues/490) | `book/sql-analysis/clauses/from.md` |
| [#489](https://github.com/kazuk/sql-fun/issues/489) | `book/sql-analysis/expressions.md` |
| [#317](https://github.com/kazuk/sql-fun/issues/317) | `book/sql-analysis/clauses/from.md` |

## Specification vocabulary review (PR #953)

The review request at
[PR #953](https://github.com/kazuk/sql-fun/pull/953#issuecomment-6598020652)
applies to every
manual page added or rewritten by this PR. The intended SQL-reference reader is
a query/schema author deciding whether analysis supports their SQL and what
input types, result descriptions and diagnostics to expect. Source code and
maintained tests remain evidence, not specification terminology.

Reviewed the landing page, all statement/clause/expression pages, navigation,
and the changed installation, CLI, diagnostics and protocol API pages. Replaced
Rust inspection calls and representation details with observable input/result
types, lookup rules, schema effects and failure boundaries. The existing issue
links and dialect/version limitations remain. Public Rust setup and catalog
usage moved to rustdoc on the exported `ParameterTypeHintWrite`,
`ParameterConstraints`, `ConflictInfoRead`, `ViewWriteEvaluation`,
`RuleInfoRead`,
`RuleInfoWrite` and `TriggerReferences` APIs. Their documentation serves library
implementers, not SQL syntax readers. No separate manual API page is added.

Evidence continues to be the source and maintained tests recorded above. In
particular, `sem/parameter_constraints.rs` supplies the inspection contracts;
`sem/scalar_expr/{case_expr,min_max,array}.rs` establish common-type behavior
and the difference between unknown literals and unresolved parameter types.
`ParameterConstraints::allows` checks assignments without setting hints;
`ParameterConstraints::result_column` and `col_type()` describe library result
inspection, so they no longer explain SQL semantics in the syntax reference.
The earlier PostgreSQL 18 oracle result `text|text[]|text|text` for all-NULL
CASE/ARRAY/GREATEST/LEAST remains applicable.

Rule/trigger identity and context-layer mechanics, including `SemAst::rule()`,
`SemAst::rule_drop()`, `SemAst::trigger()`, `SemAst::trigger_drop()` and
`TriggerIdentity`, are implementation evidence. The syntax pages instead state
relation-scoped versus schema-scoped naming and visibility to subsequent schema
analysis. SELECT's #521 limitation remains documented as incomplete
result-column
reporting: `SemScalarExpr::get_column_name` and `get_column_def` are the
affected
implementation accessors. DELETE/MERGE view support still has the #589/#609
production-catalog limitations; `ViewDeleteInfoRead` and `ViewMergeInfoRead` are
extension interfaces, not SQL constructs. No analyzer behavior or maintained
test
execution paths were changed by this review revision.

For #498, `sqlite_parser/upstream/parser.rs::parse_pragma_value` emits keyword
values for ON/DELETE/DEFAULT and other PRAGMA keywords; `sem/ast_conversion.rs`
lacks
keyword literal resolution. The syntax reference now uses PRAGMA examples
rather than the internal `Literal::Keyword` representation and keeps the
separate #557 statement-analysis limitation explicit.

## Integration after #957

PR #957 merged before this reference migration. Its addition to the former
monolithic `book/sql-analysis.md` is relocated to the utility-statement section
of `book/sql-analysis/statements.md`, expressed in SQL/domain vocabulary. #891
now describes completed syntax work; remaining interpretation belongs to #955
and #956. Syntax inspection/editing APIs remain documented on their Rust types.

Evidence: `ast/postgres.rs`, `ast/default_privileges.rs` and
`ast/foreign_table.rs` perform common-AST conversion;
`ast/engine_features.rs::utility_version_tests` and the three maintained cases
in `tests/sqlast-regressions/postgres18-syntax-17.json` establish the version
diagnostic before name resolution. `ast.rs` retains the explicit unimplemented
semantic dispatch for these statements. Their syntax support therefore does not
imply privilege validation, foreign-server lookup, LIKE expansion or catalog
effects.
