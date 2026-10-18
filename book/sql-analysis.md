# SQL analysis coverage and limitations

This page is for application developers writing queries or investigating SQL
analysis diagnostics. It describes supported forms and current limitations;
installation and initial configuration are covered in [Installation](installation.md).

## Conflicting schema definitions

Schema analysis diagnoses duplicate definitions in the supplied schema files,
including conflicts with definitions from earlier files. A rejected declaration
preserves the existing definition and analysis continues with later statements.
Tables, views, sequences, and indexes share relation names in PostgreSQL;
standalone composite types also occupy that namespace. Other types and routine
overloads follow their own namespace rules. Names in different schemas remain
distinct.

Supported `IF NOT EXISTS` forms preserve existing definitions. PostgreSQL
`OR REPLACE` permits compatible view and routine replacements; it does not permit
replacing an unrelated object kind. SQLite table/view names are compared without
ASCII case distinctions, including quoted names. SQLite `CREATE INDEX` semantic
analysis and automatically generated database index names remain unsupported.
SQLite tables with repeated constraint names also remain unsupported by the
retained schema model, although SQLite itself permits them.

`DROP TABLE` is retained as schema metadata; it does not remove the definition or
free its name for another CREATE. Supply the intended schema definitions rather
than treating schema files as a migration execution history.

## SQLite parser limitations

SQLite support is under development. Its sql-fun parser currently limits
expression depth to 100. Consequently, some SQL accepted by SQLite itself may be
rejected during sql-fun parsing. This limit applies to expression trees, including
long chains of operators, not just nested parentheses. There is currently no
sql-fun setting to increase it.

UPDATE and DELETE statements with `ORDER BY` or `LIMIT` clauses are also rejected
by the inherited parser. SQLite itself supports those clauses only in builds
with `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`; enabling that option in the database
does not enable the grammar in sql-fun. There is no sql-fun setting or
preprocessing workaround for these clauses.

The parser also recognizes Turso extensions. Successful syntax parsing does not
imply that stock SQLite or `rusqlite` can execute those extensions.

## SQLite analysis coverage

With the `sqlite` feature enabled, SELECT analysis supports literal projections,
column references from derived queries and nonrecursive CTEs, join nullability,
compound queries, and CASE expressions whose result branches have the same type.
Use explicit `AS` names when stable result-column names matter.

This is not complete SQLite query validation. Arithmetic and comparison operators,
function type affinity, case-insensitive identifier binding,
operator-dependent parameter analysis, recursive CTE binding,
and full compound type checking remain incomplete.
Supported expressions in GROUP BY, ORDER BY, HAVING, LIMIT/OFFSET, and window
clauses participate in analysis. Output aliases and ordering positions follow
SQLite binding rules; compound ordering can refer to either operand's output
expressions or aliases. Full grouping and window legality checks remain incomplete.
Different CASE result types are not unified using PostgreSQL implicit casts.
Use matching identifier spelling when referring to derived-query or CTE columns.

## INSERT analysis

PostgreSQL and SQLite INSERT analysis retains the destination column bindings,
VALUES or query source, conflict policy, and RETURNING result names, types, and
nullability. Without RETURNING, the analyzed statement has no result columns.
DEFAULT VALUES and PostgreSQL per-value DEFAULT remain distinct from explicit
expressions; analysis does not evaluate database defaults or execute the INSERT.

Explicit destination columns follow the supplied order. PostgreSQL also permits
values for only the first columns when the column list is omitted; SQLite
requires values for every non-generated column in that form. Generated and
PostgreSQL identity columns are checked against DEFAULT and OVERRIDING rules.
PostgreSQL assignment conversions constrain INSERT inputs, including inputs from
SELECT sources. SQLite inputs retain SQLite's dynamic typing rather than using
PostgreSQL assignment casts; successful analysis does not guarantee that a value
passes runtime constraints, including STRICT table checks.

RETURNING resolves destination columns and expands `*`. PostgreSQL permits
`alias.*` and hides the original table name after an INSERT alias is supplied.
SQLite RETURNING uses the original table name and rejects qualified stars.
PostgreSQL INSERT CTEs expose their RETURNING columns to subsequent queries;
a CTE without RETURNING cannot be read as a relation. SQLite permits WITH before
INSERT but does not permit INSERT inside a CTE body.

DO NOTHING retains and checks conflict targets against catalog constraints and
unique indexes, including matching expression keys and identical partial-index
predicates. Named constraint targets require constraint names retained in the
schema; automatically generated database names are not inferred.
Explicit conflict collation/operator-class references retain their completed
catalog lookups, including missing and ambiguous outcomes. Custom analysis
contexts provide these lookups and modifier-aware arbiter inference through
`ConflictInfoRead`; SQL spellings are not treated as resolved identities.
Production PostgreSQL catalogs load collation and operator-class definitions from
`collations.json` and `operator_classes.json` in the explicitly selected catalog
generation. Both files are required; incomplete or malformed generations fail to
load. Name lookup honors qualification, the current search path, database encoding,
and the requested access method. `ConflictInfoRead` provides purpose-specific
`database_collation`, `collation_definition`, and `operator_class_definition` reads
for database-default provenance and qualified definitions with operator-family/type
relations. The provider selects encoding/access-method eligibility; backing master
collections remain private. Frozen catalogs retain the same read services.

SQLite lookup uses its connection-local, case-insensitive collation namespace.
BINARY, NOCASE, and RTRIM are available. API consumers must explicitly declare
custom names with `BaseContext::declare_sqlite_collation` before freezing the base;
this declaration records availability for analysis and does not install a runtime
collation or inspect a connection.

Modifier-aware arbiter eligibility and general partial-index predicate implication
still require additional catalog services and can terminate analysis at their
catalog boundary. Their implementation and production INSERT end-to-end
verification remain tracked in [#581](https://github.com/kazuk/sql-fun/issues/581).
Master lookup support alone does not establish those INSERT capabilities.
DO UPDATE remains explicitly
unimplemented
([#474](https://github.com/kazuk/sql-fun/issues/474)); it does not produce a
successful INSERT analysis, including later actions in SQLite UPSERT chains.

Ordinary view writes are analyzed from the schema declarations you explicitly
load. API consumers construct `ViewWriteEvaluation::new(&mut context)` with the
existing analysis context, then call `insert`, `update`, or `evaluate` with an
already-bound View `FullName`. Evaluation follows the View's effective semantic
Trigger/Rule references and does not register capabilities or resolve a new
search path. `ViewInsertSupport::analyze` and `ViewUpdateSupport::analyze` use
this same service. The service reports semantic problems through that context
and returns `Ok(None)` when evaluation cannot finish because of a semantic
problem. A read-only view still yields `Ok(Some(support))`; a write to that
view reports a problem. Invalid statements leave an explicitly unresolved
result so later statements can still be analyzed. `AnalysisError` is reserved
for unrecoverable failures, including internal parser/analysis contract
violations and diagnostic delivery failures. Frozen contexts store analysis results;
writable-view analysis requires an active `ParseContext`.
PostgreSQL supports automatic writes through a single eligible relation,
matching INSTEAD OF triggers, and unconditional INSTEAD rules. INSERT and UPDATE
are independent: an INSERT trigger does not permit UPDATE. SQLite requires a
matching INSTEAD OF trigger; an UPDATE OF trigger requires at least one of its
listed columns in SET. Materialized views remain read-only.

Automatic views retain direct-column aliases through nested views. DISTINCT,
GROUP BY, HAVING, window functions, set-returning projections, joins, set
operations, WITH, LIMIT and OFFSET prevent automatic writes. Computed columns
cannot be assigned, even with DEFAULT, but remain available in RETURNING.
Assignable columns retain effective base defaults and generated/identity
restrictions; ALTER VIEW column defaults override base defaults. PostgreSQL
expands view UPDATE DEFAULT before checking base generated/ALWAYS identity
columns, so these assignments are rejected through automatic views even though
the corresponding table UPDATE DEFAULT and view INSERT DEFAULT are allowed.
Trigger/rule
NEW-row fields have their own assignment semantics.

RETURNING is checked independently. An applicable INSTEAD rule requires an
unconditional replacement action with RETURNING. Conditional INSTEAD rules block
automatic writes; an INSTEAD OF trigger can still permit the operation. Additional
DO ALSO rules can coexist with automatic writes. For automatic PostgreSQL views,
ON CONFLICT inference maps aliases to real underlying arbiters. Trigger-backed
views allow targetless DO NOTHING without exposing base indexes. Rule replacement
and trigger paths with INSERT/UPDATE rules reject ON CONFLICT. SQLite views reject
UPSERT. Modifier and partial-index inference retains the limitations described
above for #581.

Load trigger/rule declarations, replacements, drops, and view defaults after the
view declaration, in schema order. Catalog changes in a later schema layer do
not mutate a frozen parent. Invalid view declarations produce structured
semantic diagnostics and leave the existing definition intact. If a catalog
reports `view_missing_write_metadata`, rebuild it from complete schema
declarations before analyzing writes. The analyzer retains write capabilities in
`InsertStatement::view_support` and `UpdateStatement::view_support`; it does not
execute trigger functions, rule actions, CHECK OPTION checks, or runtime row
constraints. Trigger/rule SQL cannot currently be rendered back through the
common AST deparser. Broader rule and ALTER VIEW processing remains tracked
separately.

### CREATE RULE definitions

PostgreSQL `CREATE RULE` analysis returns `SemAst::rule()`, retaining the rule's
resolved relation/name identity, event, replacement and INSTEAD options, boolean
condition, ordered analyzed actions, dependencies and available source location.
DO NOTHING retains an empty action list. Table and ordinary-view targets are
supported; materialized views are rejected. OLD/NEW bind to actual target
columns and are checked against the event and their enclosing query scope.
Bind parameters and unresolved definitions are rejected.

Rule names belong to their target relation. `RuleInfoRead::rule` retrieves an
exact `RuleIdentity`; `CreateTable::rules`, `CreateView::rules` and
`ViewLike::rules` expose target links whose `resolve` method yields the effective
analyzed definitions. `RuleInfoWrite` provides explicit registration, cancellation
and target-link updates, with shared defaults for layered lookup and association.
Resolving alone has no catalog effects. Applying a valid
statement registers its definition and target link in the current layer;
replacement leaves frozen parents and siblings intact.

RETURNING requires an unconditional INSTEAD rule with one returning action and
matching target column count, order and types. ON SELECT requires an ordinary
view, `CREATE OR REPLACE RULE "_RETURN"`, an unconditional INSTEAD single SELECT,
and matching result names as well as types. It replaces the view query and its
resolved output-column lineage. UPDATE/DELETE action RETURNING binds visible
rule rows;
PostgreSQL 18 destination row images in INSERT RETURNING remain a separately
tracked semantic boundary (#621), rather than binding the enclosing rule row.

Actions use the common query/INSERT/UPDATE/DELETE analyzers. Their ordinary
limitations apply, including ON CONFLICT DO UPDATE (#474). Within rules,
EXISTS (#570), quantified/row (#571), ARRAY (#572) and multi-expression (#573)
subquery semantics are explicitly rejected pending their implementations;
scalar and correlated subqueries remain supported. Exact result compatibility
for known built-in modifier-capable types (such as varchar, numeric and
timestamp, including arrays) is rejected pending type-modifier retention (#619),
even when their base types match. NOTIFY is still blocked at common syntax
conversion (#608).
No rewrite expansion, action execution or SQL rendering is performed.

PostgreSQL `DROP RULE` resolves its ON relation first and retains the exact
relation-scoped rule identity, IF EXISTS lookup outcome and dependency choice
in `SemAst::rule_drop()`. Application masks the named definition and removes its
target link in the current layer. Frozen parents and siblings remain unchanged;
re-creation replaces the cancellation. IF EXISTS accepts a missing relation or
rule as an explicit no-op. A View's required `_RETURN` SELECT rule cannot be
dropped, including with CASCADE; drop the View instead. CASCADE/RESTRICT is
retained for inspection, without executing database dependency removal.

### CREATE TRIGGER definitions

PostgreSQL and SQLite `CREATE TRIGGER` analysis produces a dedicated semantic
statement, available through `SemAst::trigger()`. It retains the resolved target,
event columns, timing, row/statement granularity, WHEN expression, applicable
constraint and transition-table options, and catalog dependencies. PostgreSQL
function references have a verified zero-input signature returning `trigger`;
arguments written in `EXECUTE FUNCTION (...)` are retained as protocol strings.
The analyzer does not execute or inspect the function body. SQLite command
bodies retain their analyzed SELECT/INSERT/UPDATE/DELETE statements in order,
including references to available OLD/NEW columns in nested expressions.

Analysis and catalog application are separate. Applying a valid definition
registers it by `TriggerIdentity` and associates its fully qualified name with
the target table or view. PostgreSQL identities include the owning relation;
SQLite identities use the trigger schema and case-insensitive name. A TEMP
SQLite trigger is registered in `temp`, even when its target is in `main`.
`IF NOT EXISTS` preserves the existing definition; PostgreSQL `OR REPLACE`
replaces an eligible existing definition without adding a second association.

Use a table's `triggers()` or an ordinary view's `triggers()` to read the stored
names. `TriggerReferences::resolve(context, target)` resolves them through the
same effective context as direct `BaseParseContext::trigger(identity)` lookup.
Changes in a child schema layer preserve frozen parents and sibling contexts.
Registering these definitions does not itself calculate view writability.

SQLite accepts unknown `UPDATE OF` column names. Each event column records
whether lookup succeeded and its type when known; an unknown name is not a
fabricated column binding. Trigger expressions and SQLite body commands share
the existing expression/DML support limits. SQLite RAISE expression analysis
([#495](https://github.com/kazuk/sql-fun/issues/495)) and whole-row expressions
such as PostgreSQL `WHEN (NEW IS NOT NULL)`
([#614](https://github.com/kazuk/sql-fun/issues/614)) remain unsupported.
Unsuccessful analysis does not register a trigger or attach its name to a target.
PostgreSQL `DROP TRIGGER` resolves the ON relation and its relation-scoped trigger
identity. `SemAst::trigger_drop()` exposes the resolved identity, dependency
behavior and whether lookup found a definition. `IF EXISTS` retains an explicit
no-op outcome for a missing trigger or relation. Applying a successful drop
masks inherited definitions and removes the target reference in the current
layer; re-creation replaces the mask.

SQLite `DROP TRIGGER` uses the schema-scoped trigger namespace, comparing ASCII
identifiers without case sensitivity. An unqualified name searches TEMP, main,
and the explicitly configured attached-schema order. Configure attached schemas
in their SQLite ATTACH order: the earliest attached database wins when names
collide across attachments. A qualified name selects that schema only. The
resolved trigger supplies its owning relation; no ON relation or dependency
behavior is invented. `IF EXISTS` retains an explicit no-op when lookup finds
no definition. Application masks inherited definitions and removes the target
link in the child, preserving parent and sibling catalogs.

Deferrable unique-key catalog definitions, recursive CTE binding, and expressions
already unsupported by the SELECT analyzer remain unsupported. Star expansion
supports tables, ordinary views, derived queries, nonrecursive CTEs, and joins;
merged columns in FULL JOIN
stars remain unimplemented. The PostgreSQL parser currently accepts PostgreSQL 17
grammar, so PostgreSQL 18's RETURNING OLD/NEW alias syntax is unavailable.
These limitations can terminate analysis; parsing a statement does not establish
that its semantics are supported.

## Parameter analysis

Numbered parameters (`$1` in PostgreSQL and `?1` in SQLite) can be used across
supported nonrecursive CTEs and the outer statement, including INSERT sources,
RETURNING, and conflict predicates. Repeated uses of a parameter
must have compatible SQL type requirements; conflicts produce diagnostics.
A parameter used only by `IS NULL` has no particular type requirement. Explicit
SQL casts constrain accepted input types. SQLite casts follow SQLite semantics
and do not require PostgreSQL cast rules.

PostgreSQL analysis accounts for implicit casts in operator and ordinary function
calls. A parameter or a parameter-dependent result column may have several
admissible types.

Parameters in supported ORDER BY, GROUP BY, HAVING, DISTINCT ON, LIMIT/OFFSET,
aggregate FILTER/ordering, and window expressions participate in analysis,
including nested queries. Parameter-dependent RANGE ordering and custom RANGE
operator classes are not fully analyzed.
PostgreSQL output aliases are available as standalone ORDER BY/GROUP BY items;
GROUP BY gives source columns precedence, and HAVING uses source names.
Missing and ambiguous references produce diagnostics. Successful name binding
does not imply complete aggregate or window legality checking.

Concrete PostgreSQL function signatures are supported; unresolved polymorphic
signatures remain unsupported. Type unification of unresolved inputs in CASE,
COALESCE, and indirection is unimplemented and can terminate analysis.

## DELETE analysis

PostgreSQL and SQLite DELETE analysis retains the qualified target, WITH
queries, predicate bindings and RETURNING columns. PostgreSQL USING sources,
ONLY/inheritance and positioned `WHERE CURRENT OF` are represented explicitly.
Analysis does not delete rows or remove schema definitions. Without RETURNING,
a DELETE has no result columns. Invalid DELETE SQL produces inspectable analysis
problems; a missing destination retains its requested name and an explicit failed
lookup outcome. Read-only destinations retain their bindings and capability decision.
Analyzer failures remain separate from SQL diagnostics.

PostgreSQL RETURNING can reference the target and USING sources. Unqualified
`*` expands only the target; `source.*` expands the named source. A target alias
hides the original table name. SQLite RETURNING instead uses the original table
name, does not expose the target alias, and rejects qualified stars. Explicit
SQLite INDEXED BY hints must name an index of the target.

PostgreSQL DELETE CTEs expose RETURNING columns, including explicit CTE column
aliases, to later queries. A DELETE CTE without RETURNING can be unreferenced,
but cannot be read as a relation. Data-modifying CTEs must appear at the top
level. SQLite permits WITH before DELETE, but not DELETE inside a CTE body.

The configured engine version governs PostgreSQL RETURNING `old`/`new` names:
these implicit row references are available starting with PostgreSQL 18.
Explicit source aliases named `old` or `new` remain ordinary source bindings.
The old image retains target-column nullability; the new image is nullable for
DELETE. The current PostgreSQL parser does not accept PostgreSQL 18's
`RETURNING WITH (OLD AS ..., NEW AS ...)` output-alias syntax, even when the
configured engine is PostgreSQL 18.

API consumers can supply DELETE-specific view capabilities through
`ViewDeleteInfoRead`: deletability and RETURNING support are independent of
INSERT/UPDATE capabilities. Materialized views are read-only. The bundled
catalog still lacks the trigger/rule metadata needed to establish ordinary-view
DELETE capabilities, so its view-capability lookup remains unimplemented.
Production catalog support and integration tests are tracked in
[#589](https://github.com/kazuk/sql-fun/issues/589).
Positioned DELETE uses explicitly supplied current-session cursor facts; analysis
does not check runtime cursor position or lifetime. The general SQLite expression
and type-affinity limitations described above also apply to DELETE expressions.

## MERGE analysis

PostgreSQL MERGE analysis retains its target, USING source, ON condition and
ordered WHEN actions. PostgreSQL 15 and 16 support MATCHED and NOT MATCHED
actions. PostgreSQL 17 adds NOT MATCHED BY SOURCE, RETURNING and MERGE inside
nonrecursive data-modifying CTEs. The configured engine version governs these
features; SQLite does not support MERGE.

MATCHED conditions and UPDATE values can reference both target and source.
NOT MATCHED (BY TARGET) can reference only the source; NOT MATCHED BY SOURCE
can reference only the target. These restrictions also apply to correlated
subqueries. USING subqueries cannot correlate to the target. An unconditional
WHEN makes later WHEN clauses of the same match kind unreachable. INSERT is
legal only for NOT MATCHED BY TARGET; UPDATE and DELETE require a target row.
DO NOTHING is legal for every match kind. WITH RECURSIVE is unsupported.

RETURNING belongs to the whole MERGE. It can reference target and source columns
and `merge_action()`, including inside RETURNING subqueries. Unqualified `*`
expands source columns before target columns; qualified stars expand the named
record. A source row can be absent when an action runs for NOT MATCHED BY SOURCE.
Starting with PostgreSQL 18, implicit `old` and `new` references expose target row
images. The old image is nullable when INSERT is possible, and the new image is
nullable when DELETE is possible. Explicit relation aliases named `old` or `new`
take precedence. The PostgreSQL 17 parser cannot accept PostgreSQL 18's
`RETURNING WITH (OLD AS ..., NEW AS ...)` syntax. It also normalizes plain
`NOT MATCHED` and `NOT MATCHED BY TARGET` to the same syntax node, so analysis
cannot diagnose use of the explicit `BY TARGET` spelling against PostgreSQL
15 or 16; that spelling requires PostgreSQL 17.

A MERGE CTE exposes only RETURNING columns, with any explicit CTE column aliases.
A MERGE CTE without RETURNING may be unreferenced but cannot be read as a
relation. Data-modifying CTEs must occur at the top level. Parameters across ON,
WHEN conditions, action values and RETURNING participate in one analysis.
The ordinary expression, subquery and catalog limitations described above also
apply to MERGE; analysis does not execute its actions.

API consumers supply view-target facts through `ViewMergeInfoRead`. PostgreSQL
17 and later permit automatically updatable views without INSTEAD OF triggers,
or views with an INSTEAD OF trigger for every action type named by the MERGE.
Views with rules and materialized views are rejected. Effective writable-column
metadata governs action assignments. These facts are independent of ordinary
DML capability flags. The bundled production catalogs lack this metadata;
production view-target support and integration tests are tracked in
[#609](https://github.com/kazuk/sql-fun/issues/609). Custom contexts can provide it.
