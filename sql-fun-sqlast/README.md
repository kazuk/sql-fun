# sql-fun-sqlast

Semantic analysis for PostgreSQL and SQLite used by [sql-fun](https://crates.io/crates/sql-fun).

Rust applications can also use the crate directly. `parse_statements(sql, dialect)`
returns an iterator of per-statement results in the public `sql_fun_sqlast::ast`
namespace. It handles statement boundaries, retains unresolved names, and requires
no schema or metadata files. Enable the `sqlite` feature for SQLite input.

The common AST provides read-only WITH/CTE inspection and checked CTE insertion
for application read/write queries. See the crate API documentation for parsing,
error handling, and mutation examples. The existing semantic analysis APIs remain
available separately.

See the [installation prerequisites](../book/installation.md) for the native
compiler and clang requirements when building from source.
