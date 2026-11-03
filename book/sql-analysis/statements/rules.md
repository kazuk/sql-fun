# CREATE RULE / DROP RULE

## Overview

PostgreSQL `CREATE RULE` analysis checks the target relation, rule name, event,
replacement and INSTEAD options, boolean condition, and actions in written
order. DO NOTHING has no action statements. Table and ordinary-view targets are
supported; materialized views are rejected. OLD/NEW bind to actual target
columns and are checked against the event and their enclosing query scope. Bind
parameters and unresolved definitions are rejected.

Rule names belong to their target relation. A valid definition becomes available
for subsequent schema analysis. OR REPLACE replaces the rule for that relation.

RETURNING requires an unconditional INSTEAD rule with one returning action and
matching target column count, order and types. ON SELECT requires an ordinary
view, `CREATE OR REPLACE RULE "_RETURN"`, an unconditional INSTEAD single
SELECT, and matching result names as well as types. It replaces the view query
and its resolved output-column lineage. UPDATE/DELETE action RETURNING binds
visible rule rows; PostgreSQL 18 destination row images in INSERT RETURNING
remain a separately tracked semantic boundary (#621), rather than binding the
enclosing rule row.

Actions use the common query/INSERT/UPDATE/DELETE analyzers. Their ordinary
limitations apply, including ON CONFLICT DO UPDATE (#474). Within rules, EXISTS
(#570), quantified/row (#571) and multi-expression (#573) subquery semantics are
explicitly rejected pending their implementations. Scalar, correlated and ARRAY
subqueries use the shared query-expression support rules. Exact result
compatibility for known built-in modifier-capable types (such as varchar,
numeric and timestamp, including arrays) is rejected because complete
type-modifier checking is not implemented (#619), even when their base types
match. NOTIFY actions remain unsupported (#608). No rewrite expansion, action
execution or SQL rendering is performed.

PostgreSQL `DROP RULE` resolves its ON relation first and retains the exact rule
name within that relation. After a successful DROP, the rule is no longer
available to subsequent statements in the same schema analysis. IF EXISTS
accepts a missing relation or rule as an explicit no-op. A View's required
`_RETURN` SELECT rule cannot be dropped, including with CASCADE; drop the View
instead. CASCADE/RESTRICT does not cause sql-fun to remove database
dependencies.

## Dialect and engine-specific behavior

Rules are PostgreSQL-specific. They are analyzed definitions, not an
implementation of PostgreSQL's runtime rule rewriter. Actions share the
[statement](../statements.md), [clause](../clauses.md) and
[expression](../expressions.md) support boundaries. See [CREATE
VIEW](create-view.md#writable-views) for how retained rules affect writable-view
analysis.

## Diagnostics

- [rule_invalid_definition](../../diagnostics/rule_invalid_definition.md): the
  target, duplicate definition, event scope, condition, action, result
  compatibility or protected _RETURN rule does not satisfy the implemented rule
  requirements.

Action analyzers can also emit their own diagnostics; resolving a rule does not
suppress those findings.
