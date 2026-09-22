# Installation

`sql-fun` supports `PostgreSQL`, `SQLite` and is designed to work with the `tokio-postgres` or `rusqlite` connector.

 SQLite support needs feature flag `sqlite`

`sql-fun` is a procedural macro crate. You can install it using `cargo add`:

```console
cargo add sql-fun
cargo install sql-fun-cli sql-fun-server
cargo install --locked cargo-expand
```

```console
cargo add sql-fun --features "sqlite"
cargo install sql-fun-cli sql-fun-server --features "sqlite"
cargo install --locked cargo-expand
```

`sql-fun-cli` for CLI tool for `sql-fun`. `sql-fun-server` provides cache for schema objects.

`sql-fun check` requires `cargo-expand` at runtime to extract queries from expanded
Rust source. It is required for both PostgreSQL and SQLite and is not installed
automatically with `sql-fun-cli`. Ensure Cargo's binary directory is on `PATH`
and verify the installation before running `sql-fun check`:

```console
cargo expand --version
```

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


If you're starting a new project, you may also need to install the required dependencies:

```console
cargo add tokio --futures full
cargo add tokio-postgres futures-util
```

## Setting environment variables

- `SQL_FUN_SCHEMA` : optional and default is `develop`
  - Used to determine which schema dump file to load and which `sql-fun` output file to generate.

Additionally, you may want to set environment variables used by `libpq` for smooth development.

- See: [PostgreSQL: Documentation: 17: 32.15. Environment Variables](https://www.postgresql.org/docs/current/libpq-envars.html)

## Providing a schema dump (optional, for schema-aware validation)

If you're working with a new database and have no existing schema,
you can manually write a DDL SQL file and place it as `schema.develop.sql`.
(develop comes from the SQL_FUN_SCHEMA environment variable.)

If you're working with an existing database, you can generate the schema dump like this:

```console
pg_dump -s > schema.develop.sql
```

If no schema file is provided, sql-fun will still work —
but schema-aware validation will be disabled, and you'll see a warning during compilation:

```text
sql-fun: schema not loaded
```

To disable this warning, you can set `schema_checking = false` in your `Cargo.toml`:

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
Use your preferred tools or workflows — whether it's manual SQL, `sqldef`, `sqlx-cli`, `golang-migrate`, or anything else.

If you want schema-aware validation, provide a schema dump using `pg_dump -s`.  
This lets `sql-fun` validate your queries at compile time, but it never assumes ownership of the schema itself.

> ⚠️ Make sure schema definition is part of your project or CI pipeline.  
> `sql-fun` focuses on code generation, not schema evolution.
