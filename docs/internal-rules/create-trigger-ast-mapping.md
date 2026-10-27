# CREATE TRIGGER syntax mapping

This checklist is for maintainers reviewing the context-free conversion in
`sql-fun-sqlast/src/ast/trigger.rs` and `ast/sqlite/dml.rs` (#598).
It describes syntax retention, not semantic support or rendering.

The audited lockfile versions are `pg_query 6.2.1` and `turso_parser 0.8.1`.
PostgreSQL's source contract is `CreateTrigStmt` / `TriggerTransition` in
`libpg_query/protobuf/pg_query.proto`; event/timing bits are defined in
`libpg_query/src/postgres/include/catalog/pg_trigger.h`. SQLite's contract is
`Stmt::CreateTrigger`, `TriggerTime`, `TriggerEvent`, and `TriggerCmd` in
`turso_parser/src/ast.rs`, adapted by `syn/sqlite/trigger_statement.rs`.

## Statement fields

| Source | Owned common destination | Coverage in `ast/trigger/tests.rs` |
| --- | --- | --- |
| PG `replace` | `CreateTrigger.replace` | `postgres_preserves_identity_flags_and_function_arguments` |
| PG `trigname`; SQLite `trigger_name` | `name: Name` (SQLite database qualification retained) | Identity and SQLite optional-clause tests |
| PG `relation`; SQLite `tbl_name` / adapter `table_name` | `relation: Relation` | Identity and SQLite optional-clause tests |
| SQLite `temporary`, `if_not_exists` | Same named flags; PG has neither clause | `sqlite_preserves_creation_and_optional_clauses` |
| PG `timing` (0, 2, 64); SQLite `time` (absent/Before/After/InsteadOf) | Shared `TriggerTiming`, optional to preserve SQLite omission | Timing/granularity and SQLite optional-clause cases |
| PG `events` (INSERT/DELETE/UPDATE/TRUNCATE) | `events: Vec<TriggerEvent>` | `postgres_preserves_all_events_and_transition_relations` |
| PG `columns`; SQLite UpdateOf names | `TriggerEvent::Update` column list | Identity and SQLite optional-clause tests |
| SQLite Delete/Insert/Update | Same event concepts (plain Update has no column filter) | `equivalent_events_use_common_shapes` |
| PG `row` | Row/Statement granularity | `postgres_timing_and_granularity` |
| SQLite `for_each_row` | Row if explicit; `None` if omitted, never Statement | SQLite optional-clause cases |
| Both `when_clause` | `predicate: Option<Expr>` | Identity, constraint, optional-clause and type-retention tests |
| PG `funcname` | `TriggerBody::Function.name: Name` | Identity/function-argument test |
| PG `args` (String nodes) | Ordered string arguments, not evaluated call expressions | Identity/function-argument and empty-argument tests |
| PG `isconstraint` | Presence of `TriggerConstraint` | Identity and constraint-option cases |
| PG `constrrel` | Optional constraint `Relation` | Constraint-option cases |
| PG `deferrable`, `initdeferred` | Independent deferral flags | Constraint-option cases |
| PG `transition_rels` | Ordered owned `TriggerTransition` list | All-events/transition test |
| Transition `name`, `is_new`, `is_table` | `name`, `new`, `table` | All-events/transition test |
| SQLite `commands` | Ordered `TriggerBody::Commands` of common statement nodes | `sqlite_preserves_ordered_common_command_payloads` |

PG raw parsing already normalizes absent FOR EACH to statement granularity,
EXECUTE FUNCTION/PROCEDURE to the same invocation, argument token spellings to
strings, and event ordering to a bitset. The conversion does not invent those
lost distinctions. SQLite preserves omitted timing and FOR EACH ROW; later
semantics must interpret them using the selected dialect, rather than equating
omission with PostgreSQL statement-level execution.

PG `RangeVar`'s catalog/schema/relation, alias, inheritance and persistence fields
reuse the existing `Relation` conversion. Its location is a diagnostic anchor,
not stored SQL. Both ordinary and constraint trigger targets are retained without
checking existence or target kind. SQLite `QualifiedName` database/name fields
are retained; targets also reuse the existing alias handling. The trigger-name
grammar does not accept an alias. Neither dialect's trigger identity is qualified
through a search path during syntax conversion.

## SQLite command boundaries

All four parser command shapes can reuse existing common nodes. No new nested
AST architecture or missing command-shape implementation is required here.
The conversions only supply the fields present in the trigger grammar; they do
not manufacture WITH, aliases, indexes, or RETURNING clauses that are absent.

| Command and every source field | Existing representation/conversion |
| --- | --- |
| Update: `or_conflict`, `tbl_name`, `sets`, `from`, `where_clause` | `Update`: conflict flag, unqualified `Relation`, `Assignment::sqlite_sets`, `FromItem`, `Expr` |
| Insert: `or_conflict`, `tbl_name`, `col_names`, `select`, `upsert`, `returning` | `Insert`: conflict flag, relation, insert columns, `Query`, `Conflict`, `Target` |
| Delete: `tbl_name`, `where_clause` | `Delete`: relation and predicate |
| Select: `Select` | `Query::try_from` |

`sqlite_preserves_ordered_common_command_payloads` checks the order, UPDATE FROM,
multiple assignment targets, INSERT SELECT, conflict targets/predicates/actions/
chains, DELETE subquery and SELECT CASE/RAISE. The parser rejects RETURNING inside
a trigger, although the adapter exposes that field; `test_trigger_insert` checks
its retention with a directly constructed command. Type-retention coverage
parses CASTs in WHEN and every command through the public statement parser.

The nested retention audit delegates to these existing boundaries:

- Names/literals/variables and qualified names use the current owned name,
  literal and parameter conversions.
- Assignment sets retain target names and shared expression sources through
  the existing multi-assignment representation, including subqueries.
- Upsert retains index targets, sort/nulls options, target predicates,
  Nothing/Set actions, assignments, action predicates and the `next` chain.
- Queries retain WITH, compound SELECT/VALUES, result columns, FROM/join
  alternatives, predicates, grouping/HAVING, windows, ordering and LIMIT/OFFSET
  through `ast/sqlite/query.rs`.
- Expressions reuse `Expr::convert_sqlite`: literals, identifiers, qualification,
  parentheses, unary/binary, BETWEEN, NULL tests, CASE, CAST, COLLATE, calls/star
  calls, subqueries/EXISTS, IN list/select/table, LIKE, variables, RAISE,
  arrays/subscripts/field access and DEFAULT. RAISE preserves its action/message;
  this does not implement #495's semantics.
- Turso resolved Column/RowId/Register/SubqueryResult nodes are planner outputs,
  explicitly rejected by the existing expression boundary, not context-free
  SQL syntax alternatives. No parser-produced command alternative is skipped.

If further unsupported nested syntax is discovered, record its exact conversion
requirements in a separate issue. An explicit `todo!()` at that boundary is
acceptable under the scope clarification on #598; do not discard the syntax or
make the nested implementation a prerequisite of this migration. This change
adds no such unsupported boundary because the current command fields fit the
existing nodes.

## Integration and limits

`StatementKind::CreateTrigger` and its constructor are the new public
classification API. Payloads remain private, like other common statements.
`Statement::analyze` reaches an explicit `todo!()` owned by #602/#555; it cannot
register legacy `WriteDeclaration`/`ViewWriteChange` trigger metadata. Deparsing
returns the existing unsupported-node error. DROP TRIGGER and CREATE/DROP RULE
now have dedicated routes under #599–#601; the aggregate ViewWrite variant and
WriteDeclaration syntax adapter have been removed.

CREATE TRIGGER cannot be a WITH carrier or CTE body through the existing mutation
API. SQLite type retention visits WHEN first, then the command list in source
order, using the ordinary DML/query expression traversal. No source-parser data
model, generated adapter, semantic representation, or contributor workflow was
changed.
