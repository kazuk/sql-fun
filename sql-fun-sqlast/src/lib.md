# `sql-fun-sqlast`

Parse PostgreSQL and SQLite SQL into an owned, dialect-independent common AST,
inspect its WITH clauses, and insert CTE definitions using checked mutation.
The crate also supplies the semantic analysis used internally by sql-fun.

## Parse one or more statements

Choose a dialect explicitly. The iterator discovers statement boundaries using
that dialect's parser; callers do not split SQL on semicolons. Parsing common AST
statements does not read configuration or schema files and does not resolve names
against a catalog. Each successful statement owns its syntax and can outlive the
SQL string.

```rust
use sql_fun_sqlast::{ast::Dialect, parse_statements};

let sql = "SELECT ';' AS value; SELECT unknown_function(value) FROM items";
for result in parse_statements(sql, Dialect::postgres()) {
    match result {
        Ok(statement) => {
            assert!(statement.is_query());
            assert!(statement.with_clause().is_none());
        }
        Err(error) => {
            eprintln!("{}: {:?}", error, error.span().get_from(sql));
        }
    }
}
```

The supported common AST namespace is [`ast`]. Its statement categories and
read-only WITH/CTE views provide inspection without parser-specific nodes. Private
nodes retain additional syntax that does not yet have public accessors. The
common AST is unresolved: successful parsing does not establish that referenced
relations, columns, or functions exist.

An empty or comment-only input yields no results. Statement conversion failures
carry the failed statement's UTF-8 byte range and permit the iterator to continue.
Syntax failures end iteration; [`ParseStatementError::position`] reports a more
precise parser-provided location when available. Exhaust the iterator to check
the entire input. Collecting into `Vec<_>` retains per-statement errors;
collecting into `Result<Vec<_>, _>` stops at the first error by caller choice.

PostgreSQL discovers boundaries for the entire input first. Malformed input
therefore yields one syntax error covering the input, without preceding successes.
SQLite discovers boundaries incrementally and can yield earlier statements before
a later syntax error. These behaviors follow the available parser interfaces.
PostgreSQL statement ranges omit the separating semicolon; SQLite ranges can
include separators and intervening whitespace or comments.

SQLite parsing requires the `sqlite` Cargo feature. Selecting SQLite without it
yields an error. Both dialects support common AST conversion for SELECT/VALUES,
set operations, INSERT, UPDATE, DELETE, and selected DDL. Unsupported conversions
return errors. This API does not promise every statement supported by the
underlying parsers, SQL rendering, or catalog resolution.

## Insert a catalog CTE

Use [`ast::HaveWithClause`] to edit a statement's WITH clause and the checked
collection editor to choose the insertion position. Definitions own their bodies.

```rust
use sql_fun_sqlast::{ast::{CteDefinition, Dialect, HaveWithClause}, parse_statements};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let dialect = Dialect::postgres();
let body = parse_statements("SELECT id FROM users WHERE active", dialect)
    .next().unwrap()?;
let definition = CteDefinition::new("active_users", &body)?;
let mut query = parse_statements("SELECT * FROM active_users", dialect)
    .next().unwrap()?;
query.ensure_with_clause()?.insert(0, &definition)?;
assert!(query.with_clause().unwrap().find("active_users").is_some());
# Ok(())
# }
```

The same operation supports statements that already have a WITH clause. An index
of zero inserts before existing entries; the existing length appends. Existing
order and declaration attributes are preserved. Copying an inspected CTE into an
owned definition also retains column aliases, materialization, SEARCH/CYCLE, and
its body. Names supplied to the API are identifier spellings, without SQL quote
delimiters. Lookup uses parsed PostgreSQL spelling and ASCII case-insensitive
SQLite comparison. Names and column aliases cannot contain NUL. PostgreSQL
rejects empty names; SQLite accepts them. Whitespace is preserved without trimming.

Editing preserves the selected dialect. CTE definitions must come from the same
dialect as the destination; this API does not translate SQL between dialects.
Mutation validates CTE body and placement rules, including nested data-modifying
WITH clauses and SELECT INTO. Explicitly enable RECURSIVE before inserting a
definition with SEARCH/CYCLE; disabling it while such definitions remain is
rejected. Creating an editor for an absent clause does not expose an empty SQL
WITH clause: it becomes observable after a successful insertion.
PostgreSQL data-modifying CTE bodies remain representable; SQLite
CTE bodies must be queries. Mutation errors use [`ast::AstMutationError`], and a
rejected insertion leaves the destination unchanged. Name binding and dependency
resolution remain separate semantic concerns.

## Build prerequisites

Native build prerequisites are the same as for sql-fun: a C compiler, assembler,
and clang. See the repository's
[installation guide](https://github.com/kazuk/sql-fun/blob/main/book/installation.md).
