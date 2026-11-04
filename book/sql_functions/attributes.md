# Attributes

All four query/statement macros accept the SQL source and datasource options
below. Put helper attributes such as `#[sql_fragment(...)]` after the query
macro, and argument attributes on their corresponding function parameters.

| Option | Meaning |
| --- | --- |
| `sql = "..."` | Inline SQL text. |
| `sql_file = "..."` | Plain SQL file, or a structured file plus selector. |
| `datasource = "..."` | Named datasource in package metadata; omission uses the existing `default` datasource. |
| `row_type = Type` | Explicit success type when return-type inference is insufficient. |
| `error_type = Type` | Explicit error type when inference is insufficient. |
| `builder_setter = "mutable"` or `"consuming"` | Override the selected datasource's setter contract for row-building queries. |
| `builder_build_fallible = true` or `false` | Override the datasource's build contract for row-building queries. |

The last two options affect `sql_query_one`, `sql_query_opt` and
`sql_query_many`; `sql_statement` accepts them but has no row builder to configure.
See [signatures](signatures.md) for macro-specific success types and builder
requirements. Unknown option names and incorrectly typed values are errors.
Each generated function executes one SQL statement; a lockfile entry containing
multiple statements is rejected.

## Inline SQL

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C>(client: &C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

Supply one of `sql` or `sql_file`. When neither is present the macro reports an
error. Current behavior gives `sql` precedence if both are supplied; it does not
check mutual exclusion. SQL parameter notation is described under
[Parameter binding](parameter-binding.md).

## SQL files

```rust,ignore
#[sql_fun::sql_query_one(sql_file = "select_user_by_id.sql")]
async fn find_user_by_id<C>(client: &C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

Contents of `select_user_by_id.sql`:

```sql
select id, first_name, last_name from users where id = ${id}
```

Paths are read by the compiler process. Relative paths are resolved against its
working directory, not against the Rust source file's directory. Ensure the
file is present at that path during compilation; a missing file is an error.

For JSON, TOML or YAML, append `#` and a JSONPath selecting exactly one string:

```rust,ignore
#[sql_fun::sql_query_one(sql_file = "queries.toml#$.select_user_by_id.query")]
async fn find_user_by_id<C>(client: &C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

Contents of `queries.toml`:

```toml
[select_user_by_id]
query = """
select id, first_name, last_name from users where id = ${id}
"""
```

A top-level string such as `select_user_by_id = "select ..."` instead uses
`queries.toml#$.select_user_by_id`. Zero/multiple matches or a non-string value
are errors. The equivalent JSON object or YAML mapping uses the same selector.

## Datasource

A datasource selects metadata, including the SQL dialect, engine and connector;
it does not open a database connection. For example, in `Cargo.toml`:

```toml
[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "tokio-postgres"

[package.metadata.database.edge]
engine = "sqlite"
dialect = "SQLite"
connector = "rusqlite"
```

Use `datasource = "edge"` for the SQLite functions:

```rust,ignore
#[sql_fun::sql_query_one(
    datasource = "edge",
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
fn find_user_by_id(
    connection: &rusqlite::Connection, id: i64,
) -> Result<UserRow, QueryError> {}
```

Omitting `datasource` looks up the name `default`; it does not choose another
available datasource. Unknown names and a missing connector are errors. See
[installation and metadata setup](../installation.md) for schema/builtin setup
and [SQLite bootstrap](../sqlite-bootstrap.md) for its engine metadata.
