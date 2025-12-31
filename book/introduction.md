# Introduction

`sql-fun` turns SQL queries and statements into Rust functions.

It assumes that you **can write SQL and want to write SQL**.

It doesn't try to hide SQL behind abstractions or pretend it knows better than your database.
It just makes **Rust and SQL fit together**, cleanly, safely, and enjoyably.

If you're looking for an ORM: this isn't it.  
If you're writing raw SQL and wish Rust could help more: this is it.


## What is `sql-fun`

`sql-fun` is a code generator for executing SQL queries and statements.

- It runs entirely at **compile time** — there is no runtime component.
- It requires **no runtime dependencies** in your application.
- It is designed for `PostgreSQL`:
  - SQL is parsed using `pg_query`, a full `PostgreSQL` parser.
    - You can use the full range of `PostgreSQL` syntax with no limitations.
  - All SQL statements are validated at compile time.
- It does not try to model your entire database schema.
  - Instead, it focuses only on what your application actually *does*—the SQL queries it runs.
  - This keeps the tool lightweight, focused, and reliable.
- It generates code compatible with `tokio-postgres`, the primary runtime `sql-fun` targets.


## Schema-aware validation

Just place a schema dump file (e.g., `schema.develop.sql`) in the same folder as your Cargo.toml.

```console
pg_dump ... -s > schema.develop.sql
```

`sql-fun` will validate your SQL against the schema—ensuring all referenced tables and columns actually exist.

If you use multiple schemas (e.g., dev vs prod),

you can switch between them by setting the SQL_FUN_SCHEMA environment variable.

For example:

```console
SQL_FUN_SCHEMA=prod cargo check

```
will use schema.prod.sql instead.


## CRUD table generation

`sql-fun` automatically generates a `CRUD table` in TOML format.

Given this example Rust code:

```rust
#[sql_fun::sql_query_one("insert into users (name) values (${name}) returning id")]
async fn insert_user(client: &Client, name: &str ) 
  -> Result<IdRow, ApplicationError> {};

#[sql_fun::sql_query_opt("select id, name from users where id=${id}")]
async fn select_user_by_id(client: &Client, id: i64 ) 
  -> Result<Option<UserRow>, ApplicationError> {};

#[sql_fun::sql_query_opt("select id, name from users where name=${name}")]
async fn find_user_by_name(client: &Client, name: &str ) 
  -> Result<Option<UserRow>, ApplicationError> {};

#[sql_fun::sql_query_opt("delete from users u where id=${name} returning u.id")]
async fn delete_user_by_id(client: &Client, id: i64 ) 
  -> Result<Option<IdRow>, ApplicationError> {};
```

`sql-fun` will generate a `sql-fun.develop.toml` file like this:

(The `develop` part of the filename is determined by the `SQL_FUN_SCHEMA` environment variable.)

```toml
[table.users]
inserts=["insert_user"]
selects=["select_user_by_id", "find_user_by_name"]
deletes=["delete_user_by_id"]
updates=["update_user_name_by_id"]

[column.users.id]
sql_type="bigint"
type="i64"
where_by=["select_user_by_id", "delete_user_by_id"]
update_by=["insert_user"]

[column.users.name]
sql_type="varchar"
type="String"
where_by=["select_user_by_name"]
update_by=["insert_user"]

[queries.insert_user]
sql="insert into users (name) values ($1) returning id"
parameters=["name"]
results=["id"]
creates=["user"]

[queries.select_user_by_id]
sql="select id, name from users where id=$1"
parameters=["id"]
results=["id", "name"]
reads= ["users"]

[queries.find_user_by_name]
sql="select id, name from users where name=$1"
parameters=["name"]
results=["id", "name"]
reads= ["users"]

[queries.delete_user_by_id]
sql="delete from users u where id=$1 returning u.id"
parameters=["id"]
results=["id"]
deletes=["users"]

```

You can convert this TOML to JSON and process it with `jq` or other tools to generate documentation or reports for your SQL usage automatically.

Each `[queries.*]` section includes the exact SQL string that will be sent to PostgreSQL.

Unlike many ORMs, `sql-fun` makes it clear exactly what SQL will be executed—no hidden queries, no surprises.

This means your carefully written SQL is preserved—not buried in macros or lost to code generation.

Even if you switch to another tool in the future, you’ll always have a clear, structured record of your queries.

