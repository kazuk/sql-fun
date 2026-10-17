# sql-fun-sqlast

Semantic analysis for PostgreSQL and SQLite used by [sql-fun](https://crates.io/crates/sql-fun).

Rust applications can also use the crate directly. `parse_statements(sql, dialect)`
returns an iterator of per-statement results in the public `sql_fun_sqlast::ast`
namespace. It handles statement boundaries, retains unresolved names, and requires
no schema or metadata files. PostgreSQL statement syntax errors permit iteration
to continue when a trustworthy boundary is available. Inspect
`ParseStatementError::is_recoverable()` to distinguish them from terminal
boundary-discovery failures. Enable the `sqlite` feature for SQLite input.

The common AST provides read-only WITH/CTE inspection and checked CTE insertion
for application read/write queries. `Statement::deparse_sql(dialect)` renders the
complete edited tree as PostgreSQL or SQLite SQL without the original input or
catalog lookup. Numbered parameter indices, repetitions, and gaps are preserved.
Rendering requires the statement’s explicitly selected dialect and returns
structured errors for invalid or unsupported syntax. See the crate API
documentation for parsing, error handling, mutation, and rendering examples.
The existing semantic analysis APIs remain available separately.

The default-enabled `workaround_turso_9791` feature preserves SQLite type
spellings that affect declared types and CAST affinity. It takes effect only with
`sqlite`; it does not enable SQLite itself. To evaluate the upstream parser,
disable default features and enable `sqlite`. That evaluation path uses the
upstream parser directly and can change type semantics while
[Turso issue #9791](https://github.com/tursodatabase/turso/issues/9791) remains
unresolved. Cargo unifies features across dependents: every dependent must leave
the workaround disabled for the upstream path to be selected.

See the [installation prerequisites](../book/installation.md) for the native
compiler and clang requirements when building from source.

UPDATE analysis retains the destination and writable assignment bindings, FROM
sources, WHERE, and RETURNING metadata. PostgreSQL UPDATE RETURNING can supply
CTE columns; SQLite supports WITH before UPDATE, but not UPDATE inside a CTE.
DEFAULT remains distinct from an expression, and row-subquery assignments retain
the full query and selected output position. Inspect results with
`SemAst::as_update()` and `UpdateStatement` accessors. SQLite RETURNING follows
SQLite visibility rules: target aliases and FROM sources are not visible there.

For PostgreSQL `WHERE CURRENT OF`, callers establish portal bindings through
`ParseContext::cursors_mut().bind(name, &CursorInfo::new(target, updatable))`.
The optional target is a qualified relation identity; optional update eligibility
records only known static facts. `UpdateStatement::current_of()` retains the
portal name and binding, including missing bindings with an UPDATE diagnostic.
Known target mismatches and static ineligibility are diagnosed. Portal names are
session scoped and distinct from PL/pgSQL `refcursor` variable names; callers must
resolve procedural variables before supplying bindings. Cursor position and
lifetime remain execution-time concerns. Session bindings survive statement
analysis and are not part of frozen schema catalogs.

Catalog implementations provide `ViewUpdateInfoRead` independently of INSERT
capabilities, including writable columns and RETURNING support. Built-in schema
catalogs do not yet derive view trigger/rule capabilities; analysis of ordinary
view destinations remains explicitly unimplemented in those catalogs. Table
updates do not need that capability service.
