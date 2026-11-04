# `sql_query_opt`

Execute a query returning zero or one row as `Result<Option<Row>, E>`.
Multiple rows are errors.

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
