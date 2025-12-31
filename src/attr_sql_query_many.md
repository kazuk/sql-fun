This macro attribute is used to ad-hoc query returning multiple rows from the database.

# Example

```no_run
use std::convert::Infallible;
use tracing::trace;
use tokio_postgres::Client;
use futures_util::stream::try_stream::TryStreamExt;

#[sql_fun::sql_query_many("select id, name from users where users.name like ${name_like}")]
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

///
/// RowType example
///
/// The `derive_builder::Builder` crate satisfies almost requirements.
/// only add `fn builder()-> BuilderType` to fits row type.
///
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

## `sql_query_many` requirements

- require `async` function.
- The first parameter must represent the database connection.
  - `sql_query_many` uses only the parameter's name during macro expansion.
  - The parameter may be of any type, such as `Client`, `Transaction`, or a user-defined wrapper type.
  - As long as the type supports `prepare` and `query_raw` with compatible signatures, it can be used as the connection.
- The last two parameters are interpreted as the `collector` and the `handler`, in that order.
  - Their names do not matter; only their position in the function signature is used to determine their role.
  - The `handler` is called once for each row in the result set. It consumes the `collector` and returns a new one.
  - The `collector` maintains state across the query execution.
    - The `collector` parameter must be declared as `mut`.
    - Any type can be used as the `collector`; `sql_query_many` does not directly call it.
      - The generated function simply owns and passes the collector.
      - The final `collector` value is returned by the generated function.
  - The type of `handler` must be specified via generic parameters.
    - The `handler` must be a function or closure that implements `Fn` and takes two parameters: the `collector` and a row from the result set.
      - `sql_query_many` infers the row type from the generic signature as `R` in `Fn(C, R) -> Fut`.
    - The `handler` must be an `async` function and return `Result<collector, error>`.
- All other parameters are used for SQL parameter binding.

## Row type requirements

- Must implement a `fn builder() -> BuilderType` method to return a builder instance.
  - The builder must provide setter methods with names matching the SQL column names.
  - The builder must implement a `build()` method that returns the final row type instance.

# Error Type Requirements

This function returns `Result<IdRow, E>`, where `E` must:
- Implement `From<tokio_postgres::Error>` (for query execution failures).
- Implement `From<std::io::Error>` (for unexpected column mismatches in the prepare check statement).
- Be capable of handling any errors returned by the builder type.
