# `sql_query_many`

Execute a query returning zero or more rows and return the final collector
as `Result<Collector, E>`. The final two parameters are a mutable collector
and a generic handler. PostgreSQL handlers return a future; SQLite handlers
return their result synchronously.

## Example

```no_run
use std::convert::Infallible;
use tracing::trace;
use tokio_postgres::Client;
use std::future::Future;

#[sql_fun::sql_query_many(
    sql = "select id, name from users where users.name like ${name_like}"
)]
async fn select_users_by_name_like<F, Fut>(
    client: &Client,
    name_like: &str,
    mut collector: VecCollector<RowType>,
    handler: F,
) -> Result<VecCollector<RowType>, anyhow::Error>
where
    F: Fn(VecCollector<RowType>, RowType) -> Fut,
    Fut: Future<Output = Result<VecCollector<RowType>, anyhow::Error>>,
{
    // You can add code before the query executes,
    // such as trace logging, permission checks, etc.
    //
    trace!("Query is about to execute...");
    // `sql_query_many` appends the generated query execution code
    // to the end of the function body.
    // If you return early here, the query will not be executed.
}

// A mutable, fallible builder supplies the default row contract.
#[derive(derive_builder::Builder)]
struct RowType {
    id: i64,
    name: String,
}

impl RowType {
    fn builder() -> RowTypeBuilder {
        RowTypeBuilder::default()
    }
}

/// Collector example
struct VecCollector<T> {
    collected: Vec<T>,
}
impl<T> VecCollector<T> {
    pub fn new() -> Self {
        Self {
            collected: Vec::new(),
        }
    }
    /// collect method called from handler
    pub async fn collect(mut self, value: T) -> Result<Self, Infallible> {
        self.collected.push(value);
        Ok(self)
    }
    pub fn into_inner(mut self) -> Vec<T> {
        self.collected.shrink_to_fit();
        self.collected
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
