# SQL analysis coverage and limitations

This page is for application developers writing queries or investigating SQL
analysis diagnostics. It describes supported forms and current limitations;
installation and initial configuration are covered in [Installation](installation.md).

## PostgreSQL range predicates

Analysis supports `BETWEEN`, `NOT BETWEEN`, and their explicit `ASYMMETRIC`
forms:

```sql
SELECT price BETWEEN $1::numeric AND $2::numeric AS within_range
FROM products
```

Both endpoints are inclusive. Ordinary `BETWEEN` preserves the endpoint order;
it does not reorder reversed bounds. Each bound must support the corresponding
PostgreSQL comparison with the value; compatible mixed types are allowed.
Analysis checks operand references, conversions and parameter constraints and
reports a boolean result. It does not evaluate the predicate's value.

NULL follows SQL's three-valued logic: a NULL operand does not always make the
whole predicate NULL. For example, `0 BETWEEN 1 AND NULL` is false, and
`0 NOT BETWEEN 1 AND NULL` is true. Result nullability is conservative: analysis
can report nullable even for comparisons of non-null operands, and unknown
nullability when comparison information is unresolved. SQLite range predicate
analysis remains unsupported.

## PostgreSQL symmetric range predicates

Analysis supports `BETWEEN SYMMETRIC` and `NOT BETWEEN SYMMETRIC`:

```sql
SELECT price BETWEEN SYMMETRIC $1::numeric AND $2::numeric AS within_range
FROM products
```

Either endpoint order is accepted, and both endpoints are inclusive.
Operands must support the corresponding PostgreSQL comparisons; compatible
mixed types are allowed. NULL follows SQL's three-valued comparison logic.
Analysis checks operand references and parameter constraints and describes a
boolean result; it does not evaluate the predicate's value.

## Wildcard projection expansion

PostgreSQL datasources can opt in to expanding table-dependent wildcard
projections in executable SQL. Enable `rewrite_options.expand_asterisk = true`
in the [datasource configuration](installation.md). The option is disabled unless
explicitly true, and applies independently to each datasource.

For example, when the analyzed `users` table has columns `id` and `name`, this
query:

```sql
SELECT u.* FROM users AS u
```

produces executable SQL equivalent to:

```sql
SELECT u.id AS id, u.name AS name FROM users AS u
```

Expansion uses the supplied schema's column order and preserves output names,
including duplicate names. It covers table-dependent projections in nested
queries, CTEs, joins, and DML `RETURNING` clauses. Joined wildcard projections
preserve `USING` and `NATURAL JOIN` column order and merged values.

Only projections depending on a table's column set are expanded. `count(*)`,
composite-value expansion such as `(record_value).*`, and stars over fixed view,
derived-query, or table-function outputs remain unchanged. An inner
table-dependent star can be expanded while an outer star over its resulting
fixed projection remains unchanged.

The explicit projection keeps result columns stable across database environments
with added columns or dropped unreferenced columns. Every explicitly referenced
column must still exist: dropping a required column causes execution to fail.
Expansion does not make differently migrated schemas interchangeable in general.
SQLite expansion is not available.

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

## PostgreSQL ALTER COLUMN attributes

Schema analysis supports these operations on ordinary table columns:

- `SET NOT NULL` and `DROP NOT NULL`, including their effects on subsequent
  query result nullability. Dropping NOT NULL from a primary-key or identity
  column is rejected.
- `SET STATISTICS` with integer targets from `-1` through `10000`. `-1` restores
  the database default; higher targets are reduced to `10000` with a warning.
  The `DEFAULT` spelling requires an explicitly configured PostgreSQL version
  of 17 or later.
- `SET` and `RESET` for `n_distinct` and `n_distinct_inherited`. RESET also accepts
  unknown or repeated option names as no-ops, as PostgreSQL does.
- `DROP EXPRESSION [IF EXISTS]` for stored generated columns. After removal,
  INSERT and UPDATE treat the column as an ordinary column. A virtual generated
  column cannot use this operation; the current SQL parser does not accept
  PostgreSQL 18's `VIRTUAL` declaration syntax.

These operations can be combined in one ALTER TABLE statement. Analysis follows
PostgreSQL's operation phases: for example, combining SET NOT NULL and DROP NOT
NULL leaves the column NOT NULL regardless of their textual order. An invalid
operation leaves the catalog unchanged for the whole statement, and later
statements can still be analyzed. Analysis updates schema metadata; it does not
scan database rows to establish that existing values satisfy a new constraint.

Combining these operations with other ALTER commands, changing a generation
expression with `SET EXPRESSION`, changing storage/compression, generic foreign
column options, and applying column attributes through ALTER INDEX or ALTER
FOREIGN TABLE or materialized views remain unimplemented. Version-dependent
validation for custom PostgreSQL version identifiers remains unimplemented.
Inheritance and partition propagation are
also outside the currently modeled schema behavior. The semantic API exposes
completed column bindings, requested settings, validation outcomes, and statement
applicability for the implemented operations.

## PostgreSQL record-returning table functions

For a table function that returns an unnamed `record`, provide the output column
names and types explicitly:

```sql
SELECT result.row_name, result.value
FROM public.crosstab('SELECT 1, 2, 3')
AS result(row_name text, value integer);
```

The function and column types must exist in the explicitly supplied schema and
builtin metadata. For extensions such as `tablefunc`, include their schema
objects as well. Analysis uses the declared names and types for both qualified
and unqualified references; it does not execute the function or analyze SQL
contained in its string arguments.

In `ROWS FROM`, put definitions next to each record-returning function. An outer
column alias list renames the combined output by position; omitted aliases keep
their declared names. When using `WITH ORDINALITY`, put typed definitions inside
`ROWS FROM` and use an outer name-only alias list to rename columns, including
the final `bigint` ordinality column.

Typed definition lists cannot override scalar, named-composite, or OUT/TABLE
return declarations. Missing required definitions, duplicate definition names,
and unresolved declared types or collation names produce diagnostics.

Column declarations retain type modifiers separately from type identity. Builtin
character/bit lengths, numeric precision/scale, and time/interval precision are
interpreted using PostgreSQL rules; invalid arguments produce diagnostics.
Negative numeric scales and scales exceeding precision are allowed. Time
precision above six is retained as requested and interpreted as six. Extension
type modifiers are retained with an explicit unvalidated outcome; analysis does
not execute extension typmod input functions.

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
supports tables, ordinary views, derived queries, nonrecursive CTEs, and joins.
The PostgreSQL parser currently accepts PostgreSQL 17
grammar, so PostgreSQL 18's RETURNING OLD/NEW alias syntax is unavailable.
These limitations can terminate analysis; parsing a statement does not establish
that its semantics are supported.

## PostgreSQL truth predicates

`IS TRUE`, `IS FALSE`, `IS UNKNOWN`, and their `IS NOT` forms produce a
non-null boolean, even when the operand is NULL or a nullable column.
`IS UNKNOWN` tests for a NULL boolean operand. These predicates require a
boolean operand or a legal implicit conversion to boolean. A typed integer or
text value needs an explicit boolean cast; a bare NULL or valid boolean string
literal can be converted directly. Invalid boolean string literals are diagnosed.

Truth predicates constrain parameter inputs to boolean-convertible types.
Reusing a parameter in an incompatible expression produces a
parameter-constraint diagnostic. As with
other parameter analysis, admissible input types describe valid explicitly typed
bindings; analysis does not select a wire-protocol binding.

## PostgreSQL GREATEST and LEAST

`GREATEST` and `LEAST` analyze every operand and report a PostgreSQL common
result type, implicit coercions, and nullability. NULL operands are ignored;
a guaranteed non-null operand makes the result non-null. All-unknown literal
inputs resolve to `text`. Incompatible types and types without a comparison
implementation, such as `json`, produce diagnostics.

Numbered parameters participate in common-type analysis, including nested
expressions, subqueries, and nonrecursive CTEs. Several admissible input types
can produce different result types; the analysis retains these alternatives.
When every result operand is unresolved, the result domain remains open until
input assignments are supplied to `ParameterConstraints::allows`. For example,
`GREATEST($1,NULL)` admits explicitly typed integer, text, and other ordered
inputs, while an all-unknown literal expression resolves to `text`. An explicit
SQL cast can specify an operand's type. These expressions are
separate from aggregate `MIN`/`MAX` and SQLite scalar `min`/`max`.

Mixed-type selection uses PostgreSQL type categories and the preferred-type
flags in builtin cast metadata. If analysis reports missing type or comparison facts,
refresh the selected builtin catalog using the existing
[extraction workflow](cli/extract.md).

## PostgreSQL ARRAY constructors

`ARRAY[...]` retains its ordered elements and selects a common type using
PostgreSQL's type categories and implicit conversions. For example,
`ARRAY[1,2.5]` has type `numeric[]`; all-unknown strings or NULLs produce
`text[]`. When the element type is wholly unresolved, the result domain remains
open and `ParameterConstraints::allows` checks each supplied input assignment;
for example, `ARRAY[$1,NULL]` admits integer and text inputs with their
corresponding array types. SQL NULL and string literals that have no parameter
source still resolve to `text`. The constructed array is non-null even when its
elements may be NULL. Its implicit output name is `array`; an explicit alias
supplies the output name.

An empty constructor requires an explicit array cast, such as
`ARRAY[]::integer[]`. The cast also supplies the element type to nested
constructors, including nested empty arrays. Nested arrays have the same SQL
type identity as a one-dimensional array of the same element type. Analysis
reports incompatible types and missing type-category facts as diagnostics.

Arrays of anonymous records are supported. PostgreSQL subscripting yields a
generic `record` without accessible field names; use a named composite element
type when subsequent SQL needs field references.

Numbered parameters participate in constructor analysis. The result retains
admissible input and output types and their relationships; explicit casts
constrain the target type. `ARRAY(SELECT ...)` is tracked separately (#572).

## Parameter analysis

SQLAST API callers can install explicit SQL input types on a mutable context
before calling `Statement::analyze`. Import `ParameterTypeHintWrite` and call
`context.setup_typehinting(&[Some("pg_catalog.int4"), None])`: the first slot
supplies the type of `$1`, and the second leaves `$2` for normal inference.
`ParameterTypeHintRead::get_parameter_typehint` takes the SQL parameter number
unchanged; zero and numbers outside the supplied slice return `None`.
PostgreSQL `unknown` (including its qualified spelling) leaves the slot for
normal inference, like `None`.

Type hints accept SQL type names for the context's explicitly selected dialect,
then resolve them using its catalog and search path. PostgreSQL examples include
`INTEGER`, `double precision`, `character varying`, `timestamp with time zone`,
`pg_catalog.int4`, quoted names, and arrays such as `integer[2][]` or
`integer ARRAY`. Quoted identifiers preserve case; unquoted identifiers follow
SQL case folding. `float(24)` selects `real`, and `float(53)` selects
`double precision`. Bare `character` and `bit` select their type identities
without imposing column-declaration length constraints. SQLite accepts its
quoted and multiword declared type names, including backticks and brackets;
PostgreSQL aliases are not applied to SQLite names. Empty `[]` suffixes retain
the existing SQLite/Turso array type identity, while `[name]` quotes a name.

Parameter types describe client-protocol type identities. PostgreSQL parameter
OIDs carry no type modifiers, so declarations such as `varchar(100)`,
`numeric(10,2)`, `timestamp(2) with time zone`, and `interval day to second`
are rejected rather than silently losing their constraints. Declaration size
syntax is also rejected for SQLite hints.

Callers that only need to interpret spelling can use
`parse_client_type(input, &context)` from `sql_fun_sqlast::sem`. This shared API
returns a `TypeReference` without catalog lookup or search-path resolution;
syntactically valid user-defined names need not exist yet. Canonical
`FullName::from_str` and `TypeReference::from_str` retain their separate,
case-preserving spelling and serialization contracts.

Malformed or unresolved names return an error and preserve the complete previous
setup. Successful setup replaces all previous hints, including when passed an
empty slice or only `None` entries.

Create an independent `sql_fun_sqlast::context::SchemaFileContext` over the
shared `sql_fun_sqlast::context::FrozenBaseContext` catalog for each query. If
reusing a mutable context, explicitly install the next query's hints, including
an empty setup for unhinted queries. Nested queries and realized
CTEs share that query's finalized parameter order and hints. Hints assign SQL
types during name resolution; incompatible uses still produce semantic
diagnostics. They do not insert casts or change executable SQL. The accepted
spelling is independent of the named-placeholder lexer's current
`${name:ident}` hint syntax.

Numbered parameters (`$1` in PostgreSQL and `?1` in SQLite) can be used across
supported nonrecursive CTEs and the outer statement, including INSERT sources,
RETURNING, and conflict predicates. Repeated uses of a parameter
must have compatible SQL type requirements; conflicts produce diagnostics.
A parameter used only by `IS NULL` has no particular type requirement. Explicit
SQL casts constrain accepted input types. SQLite casts follow SQLite semantics
and do not require PostgreSQL cast rules.

PostgreSQL analysis accounts for implicit casts in operator and ordinary function
calls. A parameter or a parameter-dependent result column may have several
admissible types. These domains describe alternatives for explicitly typed
inputs, rather than PostgreSQL's single signature inferred by an untyped
`PREPARE`. For example, `SELECT ARRAY[$1,1]` admits an `integer` input with an
`integer[]` result and a `bigint` input with a `bigint[]` result. PostgreSQL
chooses `integer` when preparing that statement without parameter types.
`ParameterConstraints::allows` checks admissible assignments; the internal
SQLAST analysis retains alternatives without selecting a final protocol binding.
A result column's `col_type()` is absent when
several result types remain; inspect `ParameterConstraints::result_column`
for its domain. An explicit SQL cast can narrow the result type.

The public `analyze_query` API must finalize an application's input binding
contract. If SQL inference or a supported explicit input type hint does not
establish one input type, it returns `parameter_type_unresolved` as an error
and marks the statement incomplete. This applies to non-singleton and
unrestricted input domains. An empty domain instead retains its conflict
diagnostics: no input assignment satisfies those requirements.

An unresolved binding does not mean the SQL is intrinsically invalid or that
PostgreSQL could not execute it with client-supplied parameter types. PostgreSQL's
untyped `PREPARE` signature is not automatically selected as the application's
binding. Known result-column metadata remains available even when an input
binding is unresolved. For example, `SELECT $1::text AS value` has a known `text`
result but its result cast need not determine a unique source type.

For named application parameters, `SELECT ${src:text}::text AS value` explicitly
supplies the input type and completes with a `text` binding. Likewise,
`record_fun(${src:text}::text)` resolves the input binding when `record_fun` has
a `text` argument. `${src}::text` only casts the result of the parameter
expression. Use one placeholder style per input; do not mix named placeholders
with native `$1` parameters. Explicit hints still undergo validation against
all uses of the parameter.

When overloaded operations have no SQL type information for their inputs,
PostgreSQL bind-input inference considers seven default candidates: `bool`,
`int4`, `int8`, `float4`, `float8`, `text`, and `numeric`. This list is not a
preference order: several surviving candidates still leave the binding
unresolved. Unrelated catalog types do not enlarge these default candidates.
SQL context and explicit input hints can establish other types, including
arrays, composite types, user-defined types, and `bytea`. All occurrences of a
parameter contribute constraints before its binding is finalized.
`ParameterConstraints::parameter(...).constraint()` exposes the current
inference candidates. `ParameterConstraints::allows` checks explicitly supplied
types against the SQL constraints, including types outside those candidates.
Unresolved expression results are distinct from parameter placeholders. If
resolving them would require a catalog-wide type search, analysis reports
`inference_candidate_space_unsupported` and leaves their types unresolved. This
indicates a limitation of inference, not invalid SQL. Supply SQL type context or
an input type hint to avoid that search; types established by later occurrences
are considered before this diagnostic is emitted.
For `bytea`, the protocol's text value format does not change its SQL type to
`text`; use SQL context or an explicit `bytea` input hint.

Parameters in supported ORDER BY, GROUP BY, HAVING, DISTINCT ON, LIMIT/OFFSET,
aggregate FILTER/ordering, and window expressions participate in analysis,
including nested queries. Parameter-dependent RANGE ordering and custom RANGE
operator classes are not fully analyzed.
PostgreSQL output aliases are available as standalone ORDER BY/GROUP BY items;
GROUP BY gives source columns precedence, and HAVING uses source names.
Missing and ambiguous references produce diagnostics. Successful name binding
does not imply complete aggregate or window legality checking.

Concrete PostgreSQL function signatures are supported; unresolved polymorphic
signatures remain unsupported.

PostgreSQL parameter-dependent CASE expressions constrain searched WHEN
conditions to boolean and simple CASE operands through equality overloads.
Result branches use implicit common-type conversion, considering ELSE before
THEN branches in source order. Omitted ELSE retains a nullable NULL result.
Input and result domains can contain several alternatives; inspect them through
`ParameterConstraints` rather than assuming a single result type. For example,
`CASE WHEN true THEN $1 ELSE 1 END` accepts integer and bigint inputs with
corresponding numeric result alternatives.

PostgreSQL COALESCE uses the same common-type constraints, considering operands
in source order. All operands participate, even after a known non-null operand;
unreachable operands produce a warning. The analyzed expression retains its
operands and determined implicit conversions. For example, `COALESCE($1,1)`
admits integer and bigint inputs with corresponding result alternatives;
inspect `ParameterConstraints::result_column` when no single result is known.
COALESCE nullability metadata remains conservative and does not yet prove that
a non-null fallback makes the result non-null.

When every CASE result branch is unresolved, the result domain remains open
until input assignments are supplied to `ParameterConstraints::allows`. This
also applies to ARRAY and GREATEST/LEAST. All-unknown SQL literals still resolve
to `text`. SQLite CASE analysis does not apply these PostgreSQL boolean or
implicit-cast constraints; parameter-dependent SQLite result typing remains
incomplete. SQLite COALESCE retains its operands without PostgreSQL common-type
conversions; its parameter-dependent result typing remains incomplete, and its
input parameters remain unconstrained by COALESCE.

## Null-safe distinctness predicates

PostgreSQL `IS DISTINCT FROM` and `IS NOT DISTINCT FROM` analyze operand
bindings and equality-compatible parameter types. Builtin comparisons return a
non-null boolean, including comparisons with NULL. A bare NULL does not require
an equality operator or determine a parameter's input type: `$1 IS DISTINCT FROM
NULL` still needs an input binding. A typed NULL, such as `NULL::integer`,
participates in equality type checking. Explicit row pairs compare corresponding
fields and must have the same number of fields. Custom equality implementations
without a non-null result guarantee retain unknown result nullability.
Domain and named composite operand matching still has the shared limitations
tracked in [#799](https://github.com/kazuk/sql-fun/issues/799).
SQLite supports the equivalent `IS` / `IS NOT` forms and their long spellings
without PostgreSQL equality overload constraints.

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

## PostgreSQL ALTER INDEX coverage

Schema analysis supports `ALTER INDEX [IF EXISTS] name SET (...)` and
`RESET (...)` for built-in B-tree storage parameters: `fillfactor`,
`deduplicate_items`, and `vacuum_cleanup_index_scale_factor`. Settings supplied
by `CREATE INDEX ... WITH (...)` are retained for subsequent alterations.
`RESET` removes explicit overrides, including a successful no-op for an unknown
parameter name, following PostgreSQL behavior.

`ALTER INDEX [IF EXISTS] name ALTER [COLUMN] n SET STATISTICS value` supports
expression key columns, addressed by their one-based position in the index.
Ordinary key columns and included columns cannot receive index statistics.
Use `-1` to reset to the database's statistics default; values above 10000 are
lowered to 10000 with a warning. The analyzer does not choose the database's
configured default value.

Supported alterations resolve the target against the schema search path and
update the index definition used by subsequent schema statements. An invalid
statement leaves the catalog unchanged. `IF EXISTS` skips an absent target;
it does not permit a target of another relation kind.

Other access-method storage parameters, `SET TABLESPACE`, `RENAME`, extension
dependencies, `ALL IN TABLESPACE`, and `ATTACH PARTITION` remain unimplemented.
Recognizing their syntax does not imply semantic or catalog support.

## Qualified column references

PostgreSQL column analysis accepts `column`, `table_or_alias.column`,
`schema.table.column`, and `database.schema.table.column`. References bind only
to relations visible in the current query or an enclosing query. A table or
view alias hides its original relation name, even when the alias repeats the
table name: `FROM public.items AS items` allows `items.id`, but hides
`public.items.id`.

Distinct unaliased relations with the same local name can coexist across schemas:
`SELECT a.t.id, b.t.id FROM a.t, b.t` binds each column to its named relation.
The short qualifier `t.id` (and `t.*`) is ambiguous in that query, even if only
one relation contains the requested column. An unqualified column can still
resolve when exactly one relation supplies it. `SELECT *` includes both sources
in FROM order. Repeating the same relation without distinct aliases remains
invalid, as does reusing an explicit alias.

In SELECT targets, `*`, `alias.*`, and `schema.table.*` expand visible source
columns in order. PostgreSQL also expands `(composite_expression).*`, including
named composite values, table rows, and `ROW(...)` values. A composite column
can be expanded as `column.*` when no source uses that name. A row expansion
outside a projection target, or on a scalar value, produces a diagnostic.
When a composite input's type is still unresolved, the result retains an
explicit pending expansion until parameter inference determines its fields.

For PostgreSQL `FULL JOIN ... USING` and NATURAL FULL JOIN, the merged keys
appear first, followed by the remaining left and right columns. Each merged key
uses the common type of both inputs and retains the value present on an
unmatched row. SQLite keeps merged keys in the left source's column positions
and also retains the value from an unmatched right row. Qualified stars keep
each source's own columns and their outer-join nullability.

To analyze PostgreSQL database-qualified columns, explicitly set the actual
database name in the selected datasource metadata:

```toml
[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
database-name = "application"
```

`database-name` is the exact logical database identifier, without SQL quoting;
it must be nonempty and contain no NUL character. It is independent of the
metadata datasource name (`default` in this example), and is never inferred from
a connection or a query. If a database-qualified column is used without this
setting, analysis retains a `column_database_name_required` failure. A different
database name produces `column_cross_database`; PostgreSQL cross-database
column references are unsupported.

With the `sqlite` feature, SQLite uses `database.table_or_alias.column`, where
`database` is an explicitly referenced SQLite namespace such as `main`.
Identifiers are matched with SQLite's ASCII case-insensitive rules. PostgreSQL's
four-component column form is invalid in SQLite. These column-reference rules
do not extend support for schema- or database-qualified `*` expansion.

## Table renames in schema files

`ALTER TABLE … RENAME TO …` updates the analysis catalog for later statements
in explicitly supplied schema files. The new name stays in the resolved table's
schema; table, column, and result metadata remain available through the new name.
Previously analyzed views and resolved dependencies follow the renamed table.
Independent index and sequence names keep their existing names. The old table
name becomes available for a new declaration.

Analysis retains rejected renames and their diagnostics without changing the
catalog. PostgreSQL `IF EXISTS` skips an absent table. SQLite support requires the
`sqlite` feature and uses its case-insensitive identifier rules.

Two existing analysis limitations are tracked separately: unqualified foreign-key
references lack canonical target bindings and do not follow table renames
([#810](https://github.com/kazuk/sql-fun/issues/810)), and explicit SQLite
`legacy_alter_table`/`foreign_keys` PRAGMA changes are not yet modeled for rename
propagation ([#811](https://github.com/kazuk/sql-fun/issues/811)).

## Project role inventory

ALTER OWNER analysis resolves named owners against the selected datasource's
explicit [`roles` metadata](installation.md#configure-known-postgresql-roles).
For example, `roles = ["app_owner"]` allows `ALTER SCHEMA app OWNER TO app_owner`
when schema `app` exists. The analyzer retains the resolved role and updates the
schema ownership used by later analysis. Other supported ownership targets use
the same role lookup.

SQL unquoted names fold to lowercase; quoted names preserve their exact case and
escaped quotes. Configured strings are exact catalog names, not SQL expressions.
An unavailable name produces `ownership_role_not_found`; the rejected ownership
change leaves the catalog unchanged, and later statements can still be analyzed.
Omitted configuration and an explicit empty list both leave named roles
unavailable. SQL `CURRENT_ROLE`, `CURRENT_USER`, and `SESSION_USER` remain symbolic
session-dependent owners; no concrete session role is inferred. Quoted names such
as `"CURRENT_USER"` instead request a named role from the inventory.

This is offline semantic analysis of known role identities. It does not check
whether a connected user could transfer ownership or grant database permissions.

## Explicit COLLATE annotations

PostgreSQL `COLLATE` names are resolved against the loaded collation catalog,
using schema qualification or the configured search path. Missing names are
diagnosed. An annotation preserves its operand's type, nullability and output
name; an explicit column alias still takes precedence. Known PostgreSQL operand
types that do not support collation are diagnosed when their type is available.

SQLite explicit `COLLATE` expressions are checked against the analysis metadata.
Expression collation registration is not implemented yet, so every explicit name,
including `BINARY`, `NOCASE` and `RTRIM`, currently produces a not-found diagnostic.
This static check also applies to projections that SQLite could execute without
using the collation.

Analysis retains nested annotations and unresolved names. It does not yet derive
collations across expressions or check precedence and collation conflicts among
function/operator inputs, query boundaries or ordering expressions.

## Membership predicates

`IN` and `NOT IN` analyze scalar and row operands with a value list or a
subquery. PostgreSQL resolves comparison operators and implicit conversions
using the selected catalog. A scalar subquery membership requires one output
column; a row operand requires the same number of query output columns.
Incompatible widths and comparison types produce analysis diagnostics.

PostgreSQL predicates return boolean; SQLite predicates return integer. NULL
inputs can produce NULL results. `NOT IN` does not turn a NULL comparison into
true. Result nullability is conservative: analysis does not execute a query to
determine whether its result is empty or whether another list item will match.
Membership participates in the existing parameter analysis; no additional hints
are required by this syntax.
