# Top-level statements

Choose the statement you are writing or loading from a schema file. These pages
describe analysis support, not a promise that every subform accepted by the
SQL parser can be analyzed or executed.

- Queries: [SELECT](statements/select.md).
- Changes to rows: [INSERT](statements/insert.md), [UPDATE](statements/update.md),
  [DELETE](statements/delete.md), and [MERGE](statements/merge.md).
- Tables: [CREATE TABLE and CREATE TABLE AS](statements/create-table.md),
  [ALTER TABLE](statements/alter-table.md), and [DROP TABLE](statements/drop-table.md).
- Views: [CREATE VIEW](statements/create-view.md) and
  [ALTER VIEW](statements/alter-view.md).
- Indexes: [CREATE INDEX](statements/create-index.md) and
  [ALTER INDEX](statements/alter-index.md).
- Write behavior: [CREATE/DROP RULE](statements/rules.md) and
  [CREATE/DROP TRIGGER](statements/triggers.md).
- Types, routines, and operators: [CREATE TYPE](statements/create-type.md),
  [CREATE CAST](statements/create-cast.md),
  [CREATE DOMAIN](statements/create-domain.md),
  [CREATE FUNCTION](statements/create-function.md),
  [CREATE OPERATOR](statements/create-operator.md),
  [CREATE ACCESS METHOD](statements/create-access-method.md),
  [CREATE OPERATOR CLASS](statements/create-operator-class.md), and
  [ALTER FUNCTION](statements/create-function.md#alter-function).
- Other schema statements: [sequences](statements/sequences.md),
  [CREATE SCHEMA](statements/create-schema.md),
  [ALTER ... OWNER TO](statements/alter-owner.md),
  [CREATE EXTENSION](statements/create-extension.md), [SET](statements/set.md),
  [COMMENT](statements/comment.md), and [DO](statements/do.md).

## Conflicting schema definitions

Schema analysis diagnoses duplicate definitions in supplied schema files,
including conflicts with earlier files. A rejected declaration preserves the
existing definition, and analysis continues with later statements.
Tables, views, sequences, and indexes share relation names in PostgreSQL;
standalone composite types also occupy that namespace. Other types and routine
overloads follow their own namespace rules. Names in different schemas remain
distinct.

Supported `IF NOT EXISTS` forms preserve existing definitions. PostgreSQL `OR
REPLACE` permits compatible view and routine replacements; it does not permit
replacing an unrelated object kind. SQLite table/view names are compared without
ASCII case distinctions, including quoted names. SQLite `CREATE INDEX` semantic
analysis and automatically generated database index names remain unsupported.
SQLite tables with repeated constraint names also remain unsupported by schema
analysis, although SQLite itself permits them.

[DROP TABLE](statements/drop-table.md) is retained as schema metadata; it does
not remove the definition or free its name for another CREATE. Supply the
intended schema definitions rather than treating schema files as a migration
history.

The [conflicting_creation](../diagnostics/conflicting_creation.md) diagnostic
reports an object-identity conflict. Statement-specific existence diagnostics
and permitted no-op or replacement behavior are described on the statement
pages.

## PostgreSQL view removal

PostgreSQL schema analysis resolves `DROP VIEW` targets by qualification
and the search path. `IF EXISTS` skips missing targets. A visible table,
sequence, or materialized view under a requested name still causes an error.

`RESTRICT` is the default. It blocks the drop if a dependent view or
materialized view would remain. You can include dependent ordinary views
in the same `DROP VIEW` statement. `CASCADE` also removes transitively
dependent views and materialized views. Rules and triggers owned by a
removed view disappear with it. A rejected statement leaves every target
available. Subsequent statements see removed names as absent and can
recreate them.

An extension script can drop its own member views. A direct drop outside
that script is rejected while the extension still owns the view.

A column default on a surviving view can also block `RESTRICT`. Analysis
stops if `CASCADE` would need to remove such a default, an index on a
dependent materialized view, or an extension. Dependency registration
currently covers view queries and view column defaults. Other objects,
including dependencies inside SQL-standard routine bodies, need separate
catalog support.
Further dependency coverage is tracked in
[#1197](https://github.com/kazuk/sql-fun/issues/1197). Commands requiring
standalone SQL rendering still report `query_deparse` for `DROP VIEW`.

## Unimplemented SQLite and Turso statement analysis

The following SQLite/Turso statements are recognized but cannot yet be analyzed
or used to change the schema known to subsequent analysis. Using them can
terminate analysis instead of producing a structured diagnostic.

| Statement family | Missing analysis | Tracking issue |
| --- | --- | --- |
| Maintenance | ANALYZE and VACUUM | [#549](https://github.com/kazuk/sql-fun/issues/549) |
| Attached databases | ATTACH and DETACH | [#550](https://github.com/kazuk/sql-fun/issues/550) |
| Transactions | BEGIN, COMMIT, ROLLBACK, SAVEPOINT and RELEASE | [#551](https://github.com/kazuk/sql-fun/issues/551) |
| Index lifecycle | CREATE INDEX and DROP INDEX | [#552](https://github.com/kazuk/sql-fun/issues/552) |
| Index maintenance | REINDEX and OPTIMIZE | [#553](https://github.com/kazuk/sql-fun/issues/553) |
| Virtual tables | CREATE VIRTUAL TABLE | [#554](https://github.com/kazuk/sql-fun/issues/554) |
| View removal | DROP VIEW | [#556](https://github.com/kazuk/sql-fun/issues/556) |
| Connection settings | PRAGMA statements | [#557](https://github.com/kazuk/sql-fun/issues/557) |
| Materialized views | CREATE MATERIALIZED VIEW | [#558](https://github.com/kazuk/sql-fun/issues/558) |
| Named types | CREATE TYPE and DROP TYPE | [#559](https://github.com/kazuk/sql-fun/issues/559) |
| Sequences | CREATE SEQUENCE and DROP SEQUENCE | [#560](https://github.com/kazuk/sql-fun/issues/560) |
| Domains | CREATE DOMAIN and DROP DOMAIN | [#561](https://github.com/kazuk/sql-fun/issues/561) |

This inventory includes recognized Turso
extensions; it does not assert that every form is legal in stock SQLite.
PostgreSQL support for a similarly named statement does not implement these
SQLite/Turso operations. In particular, an explicit PRAGMA cannot currently
establish connection state for later analysis. Rename propagation has the
additional [PRAGMA-state limitation](statements/alter-table.md#rename-table).

## Other PostgreSQL utility statements

SQL analysis checks PostgreSQL `GRANT` and `REVOKE` declarations in schema
and extension scripts. It resolves catalog objects, routine signatures, columns
and named roles. Schema-wide declarations apply to matching objects present
when the statement is analyzed. The catalog retains declarations in order,
including grant options and membership options. Analysis does not compute
effective permissions or execute cascading revocations. SQL rendering is
tracked separately.

For `GRANT` and `REVOKE` on large objects, SQL analysis cannot resolve
the target OID or record its privilege declarations because the analysis
catalog has no large-object definitions. See
[#1177](https://github.com/kazuk/sql-fun/issues/1177).

The shipped catalogs lack privilege targets for configuration parameters,
databases, foreign-data wrappers and servers, procedural languages, and
tablespaces. Schema privileges require a retained schema definition. A
declaration targeting a missing definition produces a diagnostic. See
[#1178](https://github.com/kazuk/sql-fun/issues/1178).

PostgreSQL `CREATE FOREIGN DATA WRAPPER` is analyzed in project schema
and extension scripts. Wrapper names are database-wide. sql-fun resolves a
named handler with no input arguments and return type `fdw_handler`, and a
named validator with input arguments `text[], oid`. Unqualified routine names
use the active search path. Schema-qualified names select a specific schema.
The `NO HANDLER` and `NO VALIDATOR` clauses explicitly leave the corresponding
routine unset.

Duplicate wrapper names, repeated routine clauses, duplicate options, and
invalid routine references produce
[`foreign_data_wrapper_invalid_definition`][fdw-diagnostic].
A definition that passes these checks becomes visible to subsequent
statements in the analysis catalog, including ownership changes and privilege
declarations.
PostgreSQL checks the creator's superuser status and runs any specified
validator at execution time. Offline analysis retains the option values for
the wrapper and leaves their runtime acceptance to PostgreSQL.

The CLI analysis server currently lacks the configured `database-name` when
resolving database-qualified routine names. Use a schema-qualified routine name
in project and extension SQL. See [#1181](https://github.com/kazuk/sql-fun/issues/1181).

Direct `analyze` and `test` inputs can report the semantic outcome and also
emit `query_deparse` because executable SQL rendering for this statement
is pending. See [#1182](https://github.com/kazuk/sql-fun/issues/1182).

PostgreSQL `DROP FUNCTION` removes the selected overload from subsequent
schema and extension analysis. Supply the input argument types to select one
overload, or omit the signature when the name identifies a unique function.
`IF EXISTS` skips a missing target. An ambiguous name still produces a
diagnostic.

The default `RESTRICT` rejects removal when a view or materialized view
depends on the function. `CASCADE` removes those dependent views, including
views that query them or reference their composite row types. Analysis does
not yet check or remove other
dependents, such as triggers, rules and expression indexes. See
[#1185](https://github.com/kazuk/sql-fun/issues/1185).

PostgreSQL `ALTER EXTENSION ... ADD` and `ALTER EXTENSION ... DROP` resolve
existing objects in the analysis catalog. Supply the extension through the
configured extension loader or register its name with `CREATE EXTENSION`.
Configured scripts establish membership for supported objects created during
installation and updates. Routine signatures use exact input types. An omitted
signature requires a unique visible match.

ADD requires an object without an extension membership. DROP requires the
object to belong to the named extension and leaves its definition available.
Subsequent statements see the revised membership.

Analysis stops on TRANSFORM targets
([#1187](https://github.com/kazuk/sql-fun/issues/1187)) and on access methods,
conversions, event triggers, foreign tables and servers, operator families,
procedural languages, text search objects, and array types whose PostgreSQL
catalog identities are unavailable
([#1189](https://github.com/kazuk/sql-fun/issues/1189)).
Commands that render the submitted SQL report `query_deparse` for standalone
membership statements. Schema-file analysis can apply supported forms
([#1190](https://github.com/kazuk/sql-fun/issues/1190)).

See [PostgreSQL view removal](#postgresql-view-removal) for `DROP VIEW` behavior.

`ALTER DEFAULT PRIVILEGES` and `CREATE FOREIGN TABLE` support syntax recognition
and SQL text generation, including PostgreSQL 18's `ON LARGE OBJECTS` and
foreign-table `LIKE` forms. With an explicitly configured PostgreSQL 17 target,
those two new forms report
[incompatible_engine_version](../diagnostics/incompatible_engine_version.md).
Recognizing the syntax does not establish that SQL analysis is complete.

`ALTER DEFAULT PRIVILEGES` does not yet resolve named roles and schemas,
validate privileges and grantees, or change the analysis catalog
([#955](https://github.com/kazuk/sql-fun/issues/955)). `CREATE FOREIGN TABLE` does
not yet validate its definition, resolve its foreign server, expand `LIKE`
columns, or publish a foreign-table definition for later analysis
([#956](https://github.com/kazuk/sql-fun/issues/956)). For engine-compatible
forms, analysis can still terminate without a structured semantic diagnostic.
These limitations do not make the corresponding SQL invalid in PostgreSQL.

The CLI reports standalone PostgreSQL `CREATE SERVER` as
`unsupported_statement`, including when the referenced foreign-data wrapper
was loaded from a bundled extension
([#1306](https://github.com/kazuk/sql-fun/issues/1306)).

Standalone PostgreSQL `NOTIFY` retains its channel and optional payload during
syntax conversion, but semantic analysis is unimplemented and can terminate
analysis ([#1098](https://github.com/kazuk/sql-fun/issues/1098)). Notification
delivery and SQL rendering are not supported. For NOTIFY within a rule, see
[CREATE RULE](statements/rules.md).

## PostgreSQL text search templates

Schema analysis registers valid `CREATE TEXT SEARCH TEMPLATE` declarations
from project schemas and extension scripts. An unqualified template is
created in the first existing schema in the active search path. An
unqualified callback is resolved through the active routine search path.

`LEXIZE` is required and takes four `internal` arguments. Optional `INIT`
takes one `internal` argument. Both callbacks must return `internal`, and
their input signatures must match exactly. The options may appear in
either order. If an option is repeated, every occurrence must resolve and
the last binding is used.

SQL analysis resolves callback identities without executing the routines.
The shipped catalog omits builtin text search template definitions and
callbacks such as `pg_catalog.dsimple_lexize`. Analysis cannot resolve an
omitted callback or detect a collision with an omitted builtin template
([#1241](https://github.com/kazuk/sql-fun/issues/1241)).

Query SQL rendering for template declarations is unsupported.

## PostgreSQL text search dictionaries

Schema analysis registers valid `CREATE TEXT SEARCH DICTIONARY` declarations
from project schemas and extension scripts. An unqualified dictionary is
created in the first existing schema in the active search path. An
unqualified `TEMPLATE` reference is resolved through the active search path,
excluding the temporary schema. A qualified reference selects the named
schema. Every `TEMPLATE` occurrence must resolve, and the last occurrence
selects the template. Dictionary names must be unique within their schema.

A template without `INIT` rejects dictionary options. For a template with
`INIT`, PostgreSQL checks the remaining options when it runs that callback.
Offline analysis records the dictionary and its template without running the
callback or checking files it may load. References to builtin templates
omitted from the shipped catalog still fail to resolve
([#1241](https://github.com/kazuk/sql-fun/issues/1241)).

Query SQL rendering for dictionary declarations is unsupported.

## PostgreSQL procedural language declarations

The SQL parser recognizes `CREATE LANGUAGE` with a `HANDLER` clause,
including `CREATE TRUSTED LANGUAGE` and declarations with an inline handler
or validator. Schema loading stops when it reaches the declaration because
language registration is pending
([#1246](https://github.com/kazuk/sql-fun/issues/1246)).
Commands that render the declaration as SQL report `query_deparse`.

[fdw-diagnostic]: ../diagnostics/foreign_data_wrapper_invalid_definition.md
