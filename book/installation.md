# Installation

`sql-fun` uses `tokio-postgres` for PostgreSQL and `rusqlite` for SQLite.
SQLite support is under development and requires the `sqlite` feature.

`sql-fun` is a procedural macro crate. You can install it using `cargo add`:

```console
cargo add sql-fun
cargo install sql-fun-cli sql-fun-server
cargo install --locked cargo-expand
```

For SQLite, enable the feature on the proc-macro, CLI, and server:

```console
cargo add sql-fun --features "sqlite"
cargo install sql-fun-cli sql-fun-server --features "sqlite"
cargo install --locked cargo-expand
```

`sql-fun-cli` for CLI tool for `sql-fun`. `sql-fun-server` provides cache for
schema objects.

`sql-fun check` requires `cargo-expand` for both PostgreSQL and SQLite.
It is not installed automatically with `sql-fun-cli`. Ensure Cargo's binary
directory is on `PATH`
and verify the installation before running `sql-fun check`:

```console
cargo expand --version
```

`sql-fun check` reports a missing prerequisite and exits with an error when
`cargo-expand` is not executable on `PATH`. Install it with the command above
and add Cargo's binary directory to `PATH` before retrying. It is a prerequisite
of this command, not a Debian package installation dependency.

Specify this in your Cargo.toml as follows:

```toml
[package.metadata.database]
default = {
    engine="postgres",
    connector = "tokio-postgres"
}
```

- `default`: default database for this application
- `engine`: Can be "postgres" or "sqlite".
- `connector`: specify database connector library.
  - When `engine` is "postgres": Can be "tokio-postgres".
  - When `engine` is "sqlite": Can be "rusqlite".

You can use multiple database in application like a bellow.

```toml
[package.metadata.database]
main = {
    engine = "postgres",
    connector = "tokio-postgres"
}
edge = {
    engine = "sqlite",
    connector = "rusqlite"
}
```

`main` and `edge` using in query.

```rust
#[sql_fun::sql_query_one(
    datasource="main", // query to postgres
    sql=#"
      select u.name
        from users u
        where u.id={id}
    "#
)]
pub async fn select_user_name_by_user_id(
    client: pg::Client,
    id: u64
) -> Result<String, QueryError> {}

#[sql_fun::sql_statement(
    datasource= "edge", // execute statement to sqlite
    sql=#"insert into profile (name) values(${name})"#
)]
pub fn insert_profile(
    client: rusqlite::Connection,
    name: &str
) -> Result<usize, rusqlite::Error> {}
```

When `datasource` attribute not specified in code, default value is "default".

If you're starting a new project, you may also need to install the required
dependencies:

```console
cargo add tokio --futures full
cargo add tokio-postgres futures-util
```

## PostgreSQL extraction prerequisites

[`sql-fun extract`](cli/extract.md) includes its catalog queries, SQL templates,
and extension export script in the installed CLI. Both connection and Docker
extraction require the PostgreSQL client tools (`psql`) on `PATH`. Connection
extraction needs network access and appropriate libpq authentication/TLS settings
(for example, `sslrootcert` and `sslmode=verify-full` for CA and hostname checking).
Docker extraction additionally requires Docker CLI and daemon access.
The server administrator's extension export requires that installation's `pg_config`,
POSIX shell utilities, and GNU tar. No `hbs`, SSH integration, or sql-fun
installation is required on the database host.

## SQLite parser limitations

SQLite support is under development. SQL parsing inherits the limitations of
`turso_parser`; the current parser limits expression depth to 100
(`MAX_EXPR_DEPTH`). Consequently, some SQL accepted by SQLite itself may be
rejected during sql-fun parsing. This limit applies to expression trees, including
long chains of operators, not just nested parentheses. There is currently no
sql-fun setting to increase it.

The parser also recognizes Turso extensions. Successful syntax parsing does not
imply that stock SQLite or `rusqlite` can execute those extensions.

### SQLite analysis coverage

With the `sqlite` feature enabled, SELECT analysis supports literal projections,
column references from derived queries and nonrecursive CTEs, join nullability,
compound queries, and CASE expressions whose result branches have the same type.
Use explicit `AS` names when stable result-column names matter.

This is not complete SQLite query validation. Arithmetic and comparison operators,
function type affinity, case-insensitive identifier binding,
parameter analysis, star expansion, recursive CTE binding, and full compound
type checking remain incomplete. Parsing grouping, ordering, or window clauses
does not mean those clauses have been semantically validated. Different CASE
result types are not unified using PostgreSQL implicit casts.
Use matching identifier spelling when referring to derived-query or CTE columns.

## Setting environment variables

- `SQL_FUN_SCHEMA` : optional and default is `develop`
  - Used to determine which schema dump file to load and which `sql-fun` output
    file to generate.

Additionally, you may want to set environment variables used by `libpq` for
smooth development.

- See:
  [PostgreSQL: Documentation: 17: 32.15. Environment Variables](https://www.postgresql.org/docs/current/libpq-envars.html)

## Providing a schema dump (optional, for schema-aware validation)

If you're working with a new database and have no existing schema,
you can manually write a DDL SQL file and place it as `schema.develop.sql`.
(develop comes from the SQL_FUN_SCHEMA environment variable.)

If you're working with an existing database, you can generate the schema dump
like this:

```console
pg_dump -s > schema.develop.sql
```

If no schema file is provided, sql-fun will still work — but schema-aware
validation will be disabled, and you'll see a warning during compilation:

```text
sql-fun: schema not loaded
```

To disable this warning, you can set `schema_checking = false` in your
`Cargo.toml`:

```toml
[package.metadata.database]
default = {
    schema_checking = false
    engine="postgres"
    connector = "tokio-postgres"
}
```

## Managing Your Schema

`sql-fun` does not manage your database schema or migrations.

You are responsible for keeping your schema in sync with your application.  
Use your preferred tools or workflows — whether it's manual SQL, `sqldef`,
`sqlx-cli`, `golang-migrate`, or anything else.

If you want schema-aware validation, provide a schema dump using `pg_dump -s`.  
This lets `sql-fun` validate your queries at compile time, but it never assumes
ownership of the schema itself.

> ⚠️ Make sure schema definition is part of your project or CI pipeline.  
> `sql-fun` focuses on code generation, not schema evolution.
