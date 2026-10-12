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

The default-enabled `turso-type-syntax-workaround` feature preserves SQLite type
spellings that affect declared types and CAST affinity. It takes effect only with
`sqlite`; it does not enable SQLite itself. To evaluate the upstream parser,
disable default features and enable `sqlite`. That evaluation path uses the
upstream parser directly and can change type semantics while
[Turso issue #9791](https://github.com/tursodatabase/turso/issues/9791) remains
unresolved. Cargo unifies features across dependents: every dependent must leave
the workaround disabled for the upstream path to be selected.

See the [installation prerequisites](../book/installation.md) for the native
compiler and clang requirements when building from source.
