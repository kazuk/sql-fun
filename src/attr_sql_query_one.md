# `sql_query_one`

Execute a query returning exactly one row as `Result<Row, E>`. Zero or
multiple rows are errors.

## Example

```no_run
use tokio_postgres::Client;
#[sql_fun::sql_query_one(
    sql = "insert into users(name) values(${name}) returning id"
)]
async fn insert_user(
    client: &Client, name: &str,
) -> Result<IdRow, anyhow::Error> {}

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

## Reference

The [SQL Functions reference](https://kazuk.github.io/sql-fun/sql_functions.html) covers the complete
macro family. Use [attributes](https://kazuk.github.io/sql-fun/sql_functions/attributes.html) for
`sql`, `sql_file`, datasource and builder options,
[function signatures](https://kazuk.github.io/sql-fun/sql_functions/signatures.html) for row/error
and connector contracts, and
[parameter binding](https://kazuk.github.io/sql-fun/sql_functions/parameter-binding.html) for `${name}`
and DTO sources. PostgreSQL functions are async; SQLite functions are synchronous
and require the `sqlite` feature.

The generated code uses lockfile SQL and result-column metadata. It does not
perform an additional first-call prepare/column-shape check. The error type must
accept the connector's errors and any fallible builder/handler errors. See
[query interruption](https://kazuk.github.io/sql-fun/sql_functions/query-interruption.html) for additional
interrupt error requirements and cancellation limits.
