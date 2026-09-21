
This macro attribute is used to ad-hoc query returning a single row from the database

# Example

```no_run
use tokio_postgres::Client;
#[sql_fun::sql_query_one(sql = "insert into users(name) values(${name}) returning id")]
async fn insert_user(client: &Client, name: &str) -> Result<IdRow, anyhow::Error> {}

#[derive(Debug, derive_builder::Builder)]
struct IdRow {
   id: i32,
}

impl IdRow {
   fn builder() -> IdRowBuilder {
       IdRowBuilder::default()
   }
}
```

# Compile-time query checking

- SQL syntax checked by `pq_query`
- AST checked for parameter name and returning column

# Query Execution

- Executes the SQL query and **returns a single row**.
- Converts **named parameters** (e.g., `${name}`) into **PostgreSQL-style positional placeholders**.
- Uses the function’s return type (`Result<IdRow, E>`) to determine the row type and error type.
  - requirements described bellow
- Checking query with `prepare` (first time call only)

# Row Type Requirements

The returned row type must:
- Implement a **`builder` function** that returns a builder instance.
- Have a builder type that provides **setter methods** for all columns.
- Implement **`build()`** to construct the final instance.
- Ensure that the builder type’s error type is compatible with the function’s return error type `E`.

The `derive_builder::Builder` crate satisfies these requirements.

# Error Type Requirements

This function returns `Result<IdRow, E>`, where `E` must:
- Implement `From<tokio_postgres::Error>` (for query execution failures).
- Implement `From<std::io::Error>` (for unexpected column mismatches in the prepare check statement).
- Be capable of handling any errors returned by the builder type.
 
