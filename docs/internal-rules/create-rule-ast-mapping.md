# CREATE RULE syntax mapping

This checklist is for maintainers auditing the context-free conversion in
`sql-fun-sqlast/src/ast/rule.rs` (#600), not semantic support or rendering.
The public classification boundary is documented in `StatementKind` rustdoc.

The audited lockfile versions are `pg_query 6.2.1` and `turso_parser 0.8.1`.
The PostgreSQL contracts are `RuleStmt`, `RangeVar` and the statement types in
`pg_query/src/protobuf.rs`, the corresponding `parsenodes.h` definitions, and
`RuleStmt`, `RuleActionList`, `RuleActionMulti`, `RuleActionStmt`, `event` and
`opt_instead` in the bundled parser (`src_backend_parser_gram.c`, generated
from PostgreSQL's `gram.y`). There is no SQLite RULE grammar or conversion.

## Statement fields and alternatives

| Source | Owned common destination | Coverage in `ast/rule/tests.rs` |
| --- | --- | --- |
| `relation` | Required `Relation`, without catalog lookup or target-kind validation | Identity, field comparison and missing-target tests |
| `rulename` | `name: Name`, with parser-normalized identifier spelling | Identity, all-event and field comparison tests |
| `where_clause` | `predicate: Option<Expr>`, retaining the complete condition | Identity, nested query, field comparison and diagnostic-location tests |
| `event` | Syntax-only `RuleEvent`: SELECT, INSERT, UPDATE, DELETE | `preserves_every_event_without_catalog`, invalid-event tests |
| `instead` | `instead: bool`; ALSO and omission both parse as false | `preserves_action_mode`, field comparison tests |
| `actions` | Ordered `Vec<StatementNode>`, never `syn::Node` or semantic `WriteAction` | Ordered DML/query, action-mode, no-action and NOTIFY boundary tests |
| `replace` | `replace: bool` independently of event/body | Identity, all-event and field comparison tests |

`frontend_fields_agree_with_common_ast` compares every statement field against
raw frontend facts, including nested target fields and action cardinality.
The other tests inspect action/condition content, not merely parse success.

`RuleActionStmt` admits SELECT, INSERT, UPDATE, DELETE and NOTIFY. The first four
reuse their existing common statement payloads. NOTIFY is an explicit unsupported
nested conversion boundary owned by [#608](https://github.com/kazuk/sql-fun/issues/608).
`notify_is_an_explicit_tracked_nested_boundary` asserts the documented `todo!()`;
no preceding or following actions are returned as a partial successful rule.
Replace this boundary and test when the owned NOTIFY conversion lands. This
independent nested work is not a prerequisite for #600's statement-level migration.

The parser normalizes single actions and parenthesized multi-action lists into
the same ordered list. Empty entries in parenthesized lists are removed; DO
NOTHING, `()` and lists of empty statements produce an empty action list. An
empty common list preserves that parser fact without inventing an executable
statement. Explicit ALSO and omitted action mode are also indistinguishable in
the raw tree. Original spelling, parentheses, comments and formatting need not
survive. INSTEAD remains distinct. ON SELECT is retained even when the target
or action would fail later PostgreSQL semantic validation.

## Nested conversion audit

Conversion passes the original scanned tokens to every condition/action. RuleStmt
has no statement location field; no statement anchor is fabricated. The existing
nested converters use their own source anchors. The diagnostic-location test
checks type-name spans in both WHERE and an action (modifiers remain structured
expressions, outside the identifier's diagnostic span).

| Nested frontend fields / alternatives | Existing common conversion and destination |
| --- | --- |
| `RangeVar.catalogname`, `schemaname`, `relname` | `Relation.name: Name`, retaining all supplied components |
| `RangeVar.inh`, `relpersistence`, `alias` | `Relation.inherit`, `persistence`, optional `Alias` with name/column aliases; the rule target grammar supplies no alias |
| `RangeVar.location` | Frontend diagnostic anchor; not stored SQL or a reason to resolve a name |
| SELECT `distinct_clause`, `into_clause`, `target_list`, `from_clause`, `where_clause` | `QueryBody::Select`: distinct expressions, `Into`, ordered `Target`s, `FromItem`s, predicate |
| SELECT `group_clause`, `group_distinct`, `having_clause`, `window_clause` | Grouping alternatives/distinct flag, HAVING expression, ordered windows |
| SELECT `values_lists` | `QueryBody::Values` with every row/expression |
| SELECT `op`, `all`, `larg`, `rarg` | `QueryBody::Set` with operation, ALL and both recursive queries; NONE selects ordinary SELECT/VALUES |
| SELECT `sort_clause`, `limit_offset`, `limit_count`, `limit_option`, `locking_clause`, `with_clause` | `Query.order`, offset/limit, with-ties flag, lock strengths/wait policies/relation names, `With` |
| INSERT `relation`, `cols`, `select_stmt`, `override`, `on_conflict_clause`, `returning_list`, `with_clause` | `Insert`: Relation, ordered column/indirection assignments, optional source Query, overriding kind, Conflict, returning Targets, With |
| UPDATE `relation`, `target_list`, `where_clause`, `from_clause`, `returning_list`, `with_clause` | `Update`: Relation, ordered Assignments, predicate, FromItems, returning Targets, With |
| DELETE `relation`, `using_clause`, `where_clause`, `returning_list`, `with_clause` | `Delete`: Relation, FromItems, predicate, returning Targets, With |
| `WithClause.ctes`, `recursive`, `location` | Ordered Ctes, recursive flag; source anchor is diagnostic information |
| `CommonTableExpr.ctename`, `aliascolnames`, `ctematerialized`, `ctequery`, `search_clause`, `cycle_clause`, `location` | Cte identity, aliases, materialization choice, common CteBody, SEARCH/CYCLE syntax; source anchor used where supported |
| `CommonTableExpr.cterecursive`, `cterefcount`, `ctecolnames`, `ctecoltypes`, `ctecoltypmods`, `ctecolcollations` | PostgreSQL analysis outputs, not populated by context-free parsing; no fabricated bindings/type results |
| `ResTarget.name`, `indirection`, `val`, `location` | Assignment target/indirection/value or result expression/alias, according to grammar position; source anchor passed into nested conversion |
| `OnConflictClause.action`, `infer`, `target_list`, `where_clause`, `location` | Conflict action, inference expressions/collations/operator classes/constraint/optional predicate, assignments and update predicate; nested anchors retained where applicable |
| `WindowDef.name`, `refname`, `partition_clause`, `order_clause`, `frame_options`, `start_offset`, `end_offset`, `location` | Window identity/reference, partition/order, frame mode/bounds/exclusion and offset expressions; nested diagnostic anchors |

These conversions live in `ast/postgres/query.rs` and `ast/postgres.rs`; the
CREATE RULE converter adds no reduced expression/query representation. Nested
FROM supports relations, subqueries/lateral aliases, joins/USING, table functions,
TABLESAMPLE, JSON_TABLE and XMLTABLE through the existing common payloads.
Grouping retains expression, empty, ROLLUP, CUBE, grouping sets and simple groups.

Expressions reuse `Expr::convert_postgres` and its type-local converters:
literals, column/star/OLD/NEW references, parameters, unary/binary and quantified
operators, Boolean/NULL/truth tests, casts and type modifiers/arrays, COLLATE,
calls and their modifiers/windows, CASE, subqueries, rows/arrays/lists,
indirection/subscripts, named arguments, SQL value/default/current-of forms,
GROUPING, and SQL JSON/XML payloads. Their source-specific options remain in
common payloads; no semantic resolution is performed. Existing unsupported
nested conversions keep their explicit failure boundaries; discovering another
missing parser-produced form requires an owning issue and checklist update,
not silent omission. The NOTIFY alternative above is the missing statement
conversion identified by this audit.

## Integration and staged semantics

`StatementNode::CreateRule` and `StatementKind::CreateRule` replace only CREATE
RULE's legacy route. DROP RULE now has its dedicated route under #601; the
aggregate ViewWrite variant and WriteDeclaration syntax adapter are removed.
Semantic analysis
is an explicit `todo!()` owned by #604, which will use ParseContext; no legacy
metadata compatibility adapter is installed. Deparsing returns the existing
unsupported-node error. WITH mutation and CTE-body construction reject this
statement through their existing boundaries. SQLite type retention rejects
CREATE RULE alongside the other PostgreSQL-only statements.

Focused unit tests and `tests/create_rule_ast.rs` execute in the standard affected
and workspace nextest paths. They use no builtin catalog. Legacy semantic tests
that require CREATE RULE registration are temporarily ignored with #604 ownership;
unrelated diagnostic cases must remain executable without a CREATE RULE fixture.

The staged semantic-test gaps observed during this migration are:

| Test | Why it awaits #604 |
| --- | --- |
| `independent_write_mechanisms`: insert_rule, update_rule, rule_returning, conditional_rule, additional_rule | Each case registers a CREATE RULE through the removed legacy path |
| `automatic_rule_precedence`: conditional, additional, additional_update, instead | Rule registration is required before evaluating write precedence |
| `defaults_and_rule_drops_change_only_the_child` | Creates a parent rule before testing child cancellation/replacement |
| `invalid_view_declarations_are_structured`: target, table, duplicate | These diagnostics belong to CREATE RULE semantic analysis |
| `extracted_builtin_rules_reach_production_analysis`: postgres17, postgres18 | Analyzes production pg_settings CREATE RULE definitions |
| `builtin_bootstrap::loads_builtin_and_user_schemas`: seventeen, eighteen | Production builtin loading reaches those pg_settings rules |
| `schema_context_chain::inherited_catalog_objects_and_merged_overloads_drive_query_analysis` | Its full builtin bootstrap reaches those rules |
| `schema_context_chain::tablefunc_crosstab_call_resolves_after_explicit_extension_loading` | Its full builtin bootstrap reaches those rules before extension analysis |
| `tests::parse_adventure_works_schema` | Its full builtin bootstrap reaches those rules before the small-stack schema check |

These are 20 sqlast cases, each observed failing at the #604
semantic `todo!()`, not syntax conversion. Restore/rewrite them when #604's
semantic-definition/application model can load the rules. The other diagnostic
cases no longer share a CREATE RULE fixture; the duplicate case supplies both
rule declarations itself. Existing trigger-migration ignores are unchanged.

Workspace validation also observed three CLI initialization tests reaching the
same #604 boundary through their production-builtin `assert_semantics` check:
`initialization_preserves_and_loads_an_existing_valid_generation`,
`a_later_version_failure_preserves_the_completed_version`, and
`explicit_mixed_engine_setup_retains_both_dispatch_paths`. Each is individually
ignored with that exact reason and #604 ownership. Restore their end-to-end
semantic assertions with the other production bootstrap cases; the remaining
CLI initialization cases stay enabled. This brings the new temporary ignores
to 23 across the workspace, without modifying initialization behavior or its
contributor-facing procedures.
