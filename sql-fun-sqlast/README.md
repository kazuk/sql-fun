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

See the [installation prerequisites](../book/installation.md) for the native
compiler and clang requirements when building from source.
