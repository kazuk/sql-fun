# `sql-fun-sqlast`

Parse PostgreSQL and SQLite SQL into an owned, dialect-independent common AST,
inspect its WITH clauses, and insert CTE definitions using checked mutation.
The crate also supplies the semantic analysis used internally by sql-fun.

## Parse one or more statements

Choose a dialect explicitly. The iterator discovers statement boundaries using
that dialect's parser and boundary discovery; callers do not split SQL on
semicolons. Parsing common AST statements does not read configuration or schema
files and does not resolve names against a catalog. Each successful statement owns
its syntax and can outlive the SQL string.

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
PostgreSQL also continues after statement syntax errors when the next boundary
is trustworthy. For example, `SELECT 1; SELECT FROM; SELECT 3;` yields a successful
query, a syntax error for the second statement, and another successful query.
Invalid tokens are reported even when they do not form a recognized statement.

Inspect [`ParseStatementError::is_syntax`] for syntax classification and
[`ParseStatementError::is_recoverable`] for whether iteration can continue.
[`ParseStatementError::is_boundary_discovery`] identifies a terminal failure: no
trustworthy boundary is available for further parsing. Unclosed compound bodies
or nesting can yield such a failure after earlier statements. Unterminated
literals/comments and other lexical failures can prevent PostgreSQL from
establishing any boundaries, yielding one error for the entire input. Unknown
boundary cases terminate recovery conservatively. SQLite syntax failures remain
terminal and can follow earlier successful statements.

[`ast::Statement::span`] identifies each successful statement in the original
input, including after cloning or editing its AST. It does not describe rendered
SQL after an edit. Errors identify the attempted statement or unresolved input;
a terminal error's range may cover several apparent statements. [`ParseStatementError::position`]
reports a more precise parser-provided location when available; PostgreSQL does
not currently provide one. PostgreSQL statement ranges omit the separating
semicolon; SQLite ranges can include separators and intervening trivia.
Exhaust the iterator to check the entire input. Collecting into `Vec<_>` retains
per-statement errors; collecting into `Result<Vec<_>, _>` stops at the first error
by caller choice.

SQLite parsing requires the `sqlite` Cargo feature. Selecting SQLite without it
yields an error. Both dialects support common AST conversion for SELECT/VALUES,
set operations, INSERT, UPDATE, DELETE, and selected DDL. Unsupported conversions
return errors. This API does not promise every statement supported by the
underlying parsers or catalog resolution. Successfully converted legal syntax can
be rendered with `Statement::deparse_sql` as described below.

### SQLite parser error wording

The external `turso_parser` controls its own error strings. Common parsing with
`--no-default-features --features sqlite` (without another dependent enabling
`workaround_turso_9791`) and the legacy `SqliteDialect::parse` and
`SqliteDialect::parse_schema_snapshot` APIs may return these strings verbatim,
including implementation-oriented formatting. The legacy APIs use the external
parser even when the workaround is enabled. Treat dependency-generated wording
as an unstable parser limitation; use the applicable error classification and
recovery behavior rather than matching exact messages.

This boundary concerns dependency errors, not sql-fun's owned analysis
diagnostics. `DiagnosticMessage` explanations and structured `AnalysisProblem`
details must use readable SQL-level descriptions when existing facts suffice.
An adapter that incorporates an external error into such a diagnostic assumes
responsibility for that presentation; external origin does not exempt the
resulting diagnostic.

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
let error = query.ensure_with_clause()?.insert(0, &definition).unwrap_err();
assert_eq!(error.duplicate_name(), Some("active_users"));
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
WITH clause: the editor holds the pending clause, including its recursive choice,
until the first successful insertion commits it to the statement. Abandoning an
empty editor leaves the statement unchanged, including after a failed insertion.
Edits to an existing clause, and edits after the first insertion, apply immediately.
PostgreSQL data-modifying CTE bodies remain representable; SQLite
CTE bodies must be queries. Mutation errors use [`ast::AstMutationError`], and a
rejected insertion leaves the destination unchanged. Name binding and dependency
resolution remain separate semantic concerns.

Inspect mutation failures through [`ast::AstMutationError`] methods when deciding
how to respond. These methods identify the reason without parsing display text:

| Method | Failure identified |
| --- | --- |
| `is_unsupported_carrier()` | The destination cannot carry WITH. |
| `invalid_body_dialect()` | The CTE body kind is not allowed; returns the rejecting dialect. |
| `is_invalid_cte_placement()` | The body contains SELECT INTO or nested modifying WITH. |
| `is_dialect_mismatch()` | Definition and destination use different dialects. |
| `is_invalid_identifier()` | A CTE name or column alias violates identifier rules. |
| `duplicate_name()` | A name collides; returns the attempted definition's spelling. |
| `invalid_index()` | The insertion index is out of range; returns that index. |
| `requires_recursive()` | SEARCH/CYCLE needs the clause to remain RECURSIVE. |

Predicates return false and detail accessors return None for other reasons.
Display and Debug output remain human-readable diagnostics, not classification
formats. The API leaves recovery decisions to the caller.

## Render the current statement

[`ast::Statement::deparse_sql`] renders the entire owned tree, including inserted
CTEs, into SQL for the explicitly supplied dialect. The dialect must match the
one used for parsing. Rendering needs neither the original input nor a catalog.

```rust
use sql_fun_sqlast::{ast::{CteDefinition, Dialect, HaveWithClause}, parse_statements};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let dialect = Dialect::postgres();
let mut statement = parse_statements("SELECT id FROM selected", dialect)
    .next().unwrap()?;
let body = parse_statements("SELECT $7 AS id WHERE $7 > $2", dialect)
    .next().unwrap()?;
statement.ensure_with_clause()?.insert(0, &CteDefinition::new("selected", &body)?)?;
let sql = statement.deparse_sql(dialect)?;
assert!(sql.contains("$7"));
assert!(sql.contains("$2"));
# Ok(())
# }
```

The result is one deterministic statement with no trailing semicolon. Original
comments, whitespace, capitalization, and optional equivalent spellings are not
preserved, except SQLite type spelling and trivia that affect declared types and
casting affinity. Identifiers and literals are escaped for the selected dialect;
parentheses preserve expression and query grouping. Rendering does not establish
that unresolved objects exist or that the database will accept their types.
The server's query realization and lockfile/code generation integration are
separate from this Rust API.

Numbered placeholder identities are retained: PostgreSQL `$n` and SQLite `?n`
keep their stored indices, including repetitions and gaps, across CTE insertion.
SQLite named and anonymous placeholders are emitted as explicit `?n` using their
already assigned slots. Bind the result by those numeric slots; original SQLite
parameter names and optional type hints are metadata, not an output binding-name
contract. Rendering does not allocate new slots or infer a binding order from
traversal. Callers combining statements must supply coherent numbering before
insertion.

SELECT/VALUES/set operations, INSERT/UPDATE/DELETE, and the DDL families accepted
by common-AST conversion are covered. PostgreSQL also supports its retained
SQL/JSON and XML expressions/table forms, and MERGE inside a CTE. SQLite output
requires the `sqlite` feature and uses stock SQLite SQL; parser extensions that
are not legal SQLite constructs return errors. This is not a cross-dialect
translator or a source-preserving formatter.

SQLite type fidelity requires the default-enabled `workaround_turso_9791`
feature together with `sqlite`. The workaround does not enable SQLite by itself.
A dependency configured with `default-features = false, features = ["sqlite"]`
uses the upstream parser directly, provided no other dependent enables the
workaround through Cargo feature unification. This is an upstream-compatibility
evaluation path: `turso_parser` 0.8.1 loses type spelling that can change declared
types and CAST affinity ([upstream issue #9791](https://github.com/tursodatabase/turso/issues/9791)).
Disabling the workaround does not provide equivalent SQLite type fidelity.

Failures use [`ast::DeparseError`]. Inspect `dialect()`, `node()`, and `message()`
for context, and the following predicates for classification:

| Method | Failure |
| --- | --- |
| `is_dialect_mismatch()` | Output dialect differs from the parsed statement. |
| `is_unavailable_dialect()` | The selected dialect feature is unavailable. |
| `is_invalid_syntax()` | Retained syntax or placement is invalid in this dialect. |
| `is_unsupported()` | A represented variant has no rendering implementation. |

An error returns no partial SQL and leaves the statement unchanged. Human-readable
error text is diagnostic output; use the methods for programmatic decisions.

## Build prerequisites

Native build prerequisites are the same as for sql-fun: a C compiler, assembler,
and clang. See the repository's
[installation guide](https://github.com/kazuk/sql-fun/blob/main/book/installation.md).
