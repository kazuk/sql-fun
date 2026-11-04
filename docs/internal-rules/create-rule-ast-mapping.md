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
Semantic analysis is implemented by `sem/ast_conversion/rule.rs` over common syntax
and the shared ParseContext. `sem::CreateRule` retains the analyzed statement;
context storage receives explicit definition and target-link operations. No
legacy metadata adapter or raw frontend-node resolver is installed. Existing
writable-view consumers derive their facts from the effective analyzed rules.
Deparsing retains its unsupported-node boundary.

Focused syntax and semantic tests execute in the standard affected/workspace
nextest paths. Tests formerly ignored solely for #604 are restored, including
production PostgreSQL builtin/bootstrap and CLI initialization checks. The
`defaults_and_rule_drops_change_only_the_child` integration case still awaits
DROP RULE SQL semantics (#605); service tests cover tombstones, explicit unlink,
replacement, re-creation and frozen parent/sibling isolation meanwhile.

NOTIFY conversion remains owned by #608. Rule analysis explicitly rejects
unimplemented subquery semantics tracked by #570–#573, rather than accepting
scalar placeholders. ON CONFLICT DO UPDATE retains its existing #474 boundary.
Exact result compatibility for modifier-capable types is rejected pending
shared type-modifier retention (#619). PostgreSQL 18 INSERT RETURNING destination
row images are diagnosed pending #621. These are independent semantic
limitations, not alternate parser routes.
