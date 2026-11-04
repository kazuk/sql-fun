# DROP TRIGGER syntax mapping

This checklist is for maintainers auditing the context-free conversion in
`sql-fun-sqlast/src/ast/trigger.rs` (#599). It describes syntax retention,
not semantic analysis or rendering. The audited lockfile versions are
`pg_query 6.2.1` and `turso_parser 0.8.1`.

The PostgreSQL source contract is `DropStmt` in
`libpg_query/protobuf/pg_query.proto`. SQLite's contract is
`Stmt::DropTrigger` and `QualifiedName` in `turso_parser/src/ast.rs`, adapted
by `syn/sqlite/trigger_statement.rs` and `syn/sqlite/names.rs`.

## Complete field and alternative checklist

| Source | Owned common destination / handling | Coverage in `ast/trigger.rs` colocated conversion tests |
| --- | --- | --- |
| PG `objects` | Exactly one List target; split the final trigger identifier from the preceding ON relation components | `postgres_preserves_target_names`, invalid-target tests |
| PG nested List `items`, String `sval` | Shared `Name` conversion retains ordered identifier values; relation retains all supplied components, trigger has one | Target-name tests cover bare, schema/catalog qualification, quoted dots, escaped quotes, case folding and overqualified unresolved names |
| PG `remove_type` | `ObjectTrigger` selects `StatementNode::DropTrigger`; other kinds cannot enter the payload conversion | `rejects_other_drop_kinds`; public classification tests |
| PG `behavior` | `DropCascade` / `DropRestrict` become `DropBehavior`; undefined/absent values are rejected | `postgres_preserves_drop_options`, `rejects_unspecified_behavior` |
| PG `missing_ok` | `DropTrigger.if_exists` | Drop-option tests cross both flag values with all behavior spellings |
| PG `concurrent` | Always false in DROP TRIGGER grammar; contradictory adapter input is rejected | `rejects_contradictory_concurrent_flag` |
| SQLite `if_exists` | Same `if_exists` flag | `sqlite_preserves_names_without_inventing_relation_or_behavior` |
| SQLite `trigger_name` | `name: Name`, preserving optional database qualification | SQLite name tests |
| SQLite nested `QualifiedName.db_name`, `name` | Ordered common name components; identifier spelling/case retained by the frontend | SQLite name tests include bare, qualified, quoted and escaped names |
| SQLite nested `QualifiedName.alias` | Absent in this grammar; contradictory constructed input is rejected | `rejects_contradictory_sqlite_alias` |

PostgreSQL raw parsing appends the trigger name to the ON relation name list.
The conversion preserves the distinction between those names without catalog
access. `relation: Option<Name>` stores the explicit ON name; the parser does
not supply a `RangeVar` here. No relation attributes are manufactured.

The parser normalizes omitted drop behavior and explicit RESTRICT to
`DropRestrict`. Both remain Restrict; the common AST does not claim to recover
which spelling appeared. SQLite has no ON or CASCADE/RESTRICT clause, so its
`relation` and `behavior` are both `None`, rather than inferred values.
`equivalent_concepts_use_common_shapes` checks the shared name/IF EXISTS shape
and these dialect differences.

There are no nested expressions or statement bodies in either DROP TRIGGER
form. All nested syntax consists of the name/list nodes audited above; this
migration introduces no separately unsupported nested conversion boundary.
PostgreSQL `DropStmt` carries no source location anchor, and SQLite does not
provide node locations. Conversion therefore uses `TryFrom` without guessed
spans. Original formatting and comments are not retained.

## Integration and limits

`StatementKind::DropTrigger` and `StatementKind::drop_trigger()` are the new
public classification API. Payloads remain private, like other common
statements. `tests/drop_trigger_ast.rs` exercises public parsing/classification,
unsupported WITH/CTE placement and the existing unsupported deparse error for
both dialects. These tests and the conversion tests run in the standard
`check-test-flow` affected/workspace nextest suites (SQLite cases require its
feature).

Semantic dispatch has an explicit `todo!()` owned by #555. This statement no
longer creates, resolves or applies legacy `WriteDeclaration`/`ViewWriteChange`
metadata. All four statement migrations (#598–#601) now have dedicated routes;
the aggregate ViewWrite variant and WriteDeclaration syntax adapter are removed.
No semantic fallback is provided.

The `missing_trigger` case of `invalid_view_declarations_are_structured` in
`tests/writable_views.rs` now has a targeted ignore naming #555: it expected a
legacy missing-object diagnostic after semantic catalog lookup. It must be
rewritten or reenabled when the new DROP TRIGGER semantic definition and
registration model is implemented. Existing CREATE TRIGGER migration ignores
remain owned by their original follow-ups.

The disconnected legacy trigger constructor and variant have temporary
`dead_code` expectations naming #555; their representation is unchanged.

No parser data model, generated adapter, semantic data representation,
contributor workflow or user manual is changed. This internal checklist serves
retention review; public rustdoc covers the classification API.
