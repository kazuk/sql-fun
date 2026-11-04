# DROP RULE syntax mapping

This checklist serves maintainers auditing the context-free conversion in
`sql-fun-sqlast/src/ast/rule.rs` (#601). It records syntax retention rather than
semantic analysis or rendering. The audited Cargo.lock versions are
`pg_query 6.2.1` and `turso_parser 0.8.1`; RULE syntax is PostgreSQL-only.

The frontend contract is `DropStmt` in the bundled
`libpg_query/protobuf/pg_query.proto`. The bundled generated PostgreSQL parser
`src/postgres/src_backend_parser_gram.c` contains the actions corresponding to
`gram.y` lines 6912/6923: both append the rule identifier to the ON relation's
name list, wrap it in one object List, retain behavior, set `missing_ok` according
to IF EXISTS, and set `concurrent` false. ObjectRule and ObjectTrigger share this
frontend shape but select distinct common statements.

## Complete field and alternative checklist

| Frontend field / alternative | Common destination or validation | Coverage in `ast/rule.rs` colocated conversion tests |
| --- | --- | --- |
| `objects` | Exactly one List; final component is rule name, preceding components are the required unresolved ON relation | `postgres_preserves_target_names`, `rejects_invalid_target_cardinality`, `rejects_missing_relation_in_target`, `rejects_non_list_target` |
| Nested List `items`, String `sval` | Existing Name conversion retains every ordered identifier value; non-String/missing components are rejected | Target-name tests and `rejects_non_string_name_component` |
| `remove_type` | ObjectRule selects DropRule; other kinds and unspecified values are rejected by its payload conversion | `rejects_other_drop_kinds`, `rejects_unspecified_object_type`; public classification tests |
| `behavior` | DropCascade / DropRestrict become shared DropBehavior; unspecified/invalid values are rejected | `postgres_preserves_drop_options`, `rejects_unspecified_behavior` |
| `missing_ok` | `DropRule.if_exists` | Both values crossed with all three behavior spellings in drop-option tests |
| `concurrent` | False in this grammar; contradictory constructed input is rejected | `rejects_contradictory_concurrent_flag` |

`frontend_fields_agree_with_common_ast` also compares raw frontend fields with
the common payload for minimal and fully specified statements.

Target-name tests cover bare, schema-qualified, catalog-qualified and
parser-accepted overqualified relations, case folding, quoted dots and escaped
quotes. No catalog lookup or semantic qualification limit is imposed. The rule
identifier is a one-component Name; the ON relation is a required Name, not a
RangeVar, optional inferred relation or bound catalog identity.

Omitted behavior and explicit RESTRICT both arrive as DropRestrict. The common
AST retains Restrict without claiming to recover which spelling was used.
Quoted identifiers retain their frontend identifier value; original quoting,
formatting and comments are not rendering requirements. No event is present or
synthesized. PostgreSQL DropStmt has no location anchor, so conversion uses
TryFrom without guessed spans.

All nested syntax is covered by the List/String name mapping: DROP RULE has no
nested expressions, queries or statement body. There is no separately unsupported
nested conversion boundary. SQLite has no RULE grammar and receives no new route.
The shared DropBehavior also serves DROP TRIGGER; SQLite DROP TRIGGER still has
no ON relation or behavior, as verified by its existing conversion tests.

## Integration and staged semantics

`StatementKind::DropRule` and `StatementKind::drop_rule()` are the new public
classification API. Payloads stay private. `tests/drop_rule_ast.rs` covers parsing
without a catalog, all four trigger/rule classifications and ordinary DROP TABLE,
rejection of WITH/CTE placement, and the existing unsupported-deparse error. These
tests and focused conversion tests execute in the standard affected/workspace
nextest suites. SQLite type retention rejects this PostgreSQL-only statement.

Semantic dispatch is an explicit `todo!()` owned by #605, preserving the existing
ParseContext boundary. No legacy metadata adapter, resolution, registration or
application is provided. Deparsing and semantic behavior remain outside #601.

With all four migrations (#598–#601) present, StatementNode::ViewWrite,
StatementKind::ViewWrite, their constructors/dispatch arms, WriteDeclaration and
its frontend/deparse adapters are removed. Its now-unused trigger-declaration
helper is also removed. The View/query metadata producers in `ast/view_write.rs`
and still-owned semantic infrastructure remain intact. Disconnected legacy DROP
and rule constructors/variants have targeted dead-code expectations owned by
issues #604/#605; their data models and serialization contracts are unchanged.

The `missing` case of `invalid_view_declarations_are_structured` in
`tests/writable_views.rs` now has a targeted ignore naming #605: it expected
legacy DROP RULE missing-object diagnostics after catalog lookup. Rewrite or
reenable that case when the new semantic definition/registration model lands.
Existing sibling migration ignores remain under their original owners.

Public rustdoc records dedicated classification and removal of the aggregate.
There is no semantic user-manual expansion, generated-source edit, CI change or
contributor-workflow change.
