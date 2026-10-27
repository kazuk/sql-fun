# `sql_query_opt`

This macro attribute is used to ad-hoc query returning a optional single row
from the database

## Example

```no_run
use tokio_postgres::Client;
#[sql_fun::sql_query_opt(sql = "select id, name from users where id=${id}")]
async fn select_user_by_id(
    client: &Client, id: i64,
) -> Result<Option<UserRow>, anyhow::Error> {}
#[derive(Debug, derive_builder::Builder)]
struct UserRow {
   id: i32,
   name: String
}
impl UserRow {
   fn builder() -> UserRowBuilder {
       UserRowBuilder::default()
   }
}
```

## Compile-time query checking

- SQL syntax checked by `pq_query`
- AST checked for parameter name and returning column

## Query Execution

- Executes the SQL query and **returns a single row**.
- Converts **named parameters** (e.g., `${name}`) into
  **PostgreSQL-style positional placeholders**.
- Uses the function’s return type (`Result<IdRow, E>`) to determine the row type
  and error type.
  - requirements described bellow
- Checking query with `prepare` (first time call only)

## Row Type Requirements

The returned row type must:

- Require Option Type
- Implement a **`builder` function** that returns a builder instance.
- Have a builder type that provides **setter methods** for all columns.
- Implement **`build()`** to construct the final instance.
- Ensure that the builder type’s error type is compatible with the function’s
  return error type `E`.

The `derive_builder::Builder` crate satisfies these requirements.

## Error Type Requirements

This function returns `Result<IdRow, E>`, where `E` must:

- Implement `From<tokio_postgres::Error>` (for query execution failures).
- Implement `From<std::io::Error>` (for unexpected column mismatches in the
  prepare check statement).
- Be capable of handling any errors returned by the builder type.
