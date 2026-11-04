# Function signatures

The macro appends execution code to the function body. An empty body is enough;
existing statements run first, so an early return bypasses the query. Its name,
arguments and return type define the API your application calls. Use identifier
argument patterns. PostgreSQL functions must be `async`; SQLite functions must
be synchronous. The SQLite connector requires the `sql-fun` `sqlite` feature.

## Connection and transaction

The first argument is the connection. Its name is unrestricted; its type must
support the generated connector calls. The macro does not create a connection
or start a transaction.

For PostgreSQL, use `&tokio_postgres::Client`,
`&tokio_postgres::Transaction<'_>`, or a generic client:

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C>(client: &C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

A `Client` signature accepts ordinary client calls; a `Transaction` signature
requires a transaction value. A generic `GenericClient` accepts either. These
are Rust API restrictions: `Transaction::client()` still lets application code
access the underlying client, so they do not enforce transaction policy outside
this function.

For SQLite, `&rusqlite::Connection` accepts a connection or a transaction through
its dereference to `Connection`. `&rusqlite::Transaction<'_>` requires a
transaction argument. With interruption, use a mutable connection as described
under [Query interruption](query-interruption.md#sqlite).

```rust,ignore
#[sql_fun::sql_query_one(
    datasource = "edge", sql = "select id, name from users where id = ${id}"
)]
fn find_user(
    connection: &rusqlite::Connection, id: i64,
) -> Result<UserRow, QueryError> {}
```

Wrappers must provide compatible methods: PostgreSQL uses `query_one`,
`query_opt`, `query_raw` or `execute`; SQLite queries use `prepare` followed by
statement/row methods, and statements use `execute`.

## Return types and inference

| Macro | Successful return | Cardinality |
| --- | --- | --- |
| `sql_query_one` | `Row` | Exactly one; zero or multiple rows are errors. |
| `sql_query_opt` | `Option<Row>` | Zero or one; multiple rows are errors. |
| `sql_query_many` | `Collector` | Zero or more; returns the final collector. |
| `sql_statement` | `u64` (PostgreSQL), `usize` (SQLite) | Affected-row count, without collecting rows. |

Each return is wrapped in `Result<Success, Error>`. Without explicit attributes,
the first generic type argument of the declared return type supplies the success
type and the second supplies the error type. The name/path of that type is
ignored; aliases are not resolved.

| Declaration | Inference |
| --- | --- |
| `Result<UserRow, QueryError>` | Both types. |
| `std::result::Result<UserRow, QueryError>` | Both types. |
| `anyhow::Result<UserRow>` | Success only; supply `error_type = anyhow::Error`. |
| `QueryResult` (alias) | Neither; supply `row_type` and `error_type`. |

For `sql_query_opt`, the inferred success type is `Option<Row>` and its first
generic argument supplies the builder's row. When overriding `row_type` for an
alias, either `Option<Row>` or the inner `Row` can supply its builder type.
For `sql_query_many`, the row to build comes from the
handler's second argument, not from the collector return type. For
`sql_statement`, the success type is the affected-row count; no row is built.
All macros still need inferable success/error types or their explicit overrides.
The declared return must remain compatible with the generated `Result`.

```rust,ignore
type QueryResult = anyhow::Result<UserRow>;

#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}",
    row_type = UserRow,
    error_type = anyhow::Error
)]
async fn find_user_by_id<C>(client: &C, id: i64) -> QueryResult
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

## Collecting multiple rows

For `sql_query_many`, the last two arguments must be the collector and handler,
in that order. Their names do not matter. Declare the collector `mut`: the
handler consumes it and returns its next value for each row. An empty result
returns the original collector. No collector methods are called by the macro.

The handler argument must be a generic type parameter with a `Fn` bound, either
inline or in a `where` clause. Its second input identifies the row type.

| Connector | Handler contract |
| --- | --- |
| PostgreSQL | `F: Fn(Collector, Row) -> Fut`, `Fut: Future<Output = Result<Collector, E>>` |
| SQLite | `F: Fn(Collector, Row) -> Result<Collector, E>` |

The handler may be supplied as a closure satisfying that bound. A concrete
handler type or a reference `&F` in the generated function's declaration does
not satisfy the macro's signature parser. Keep condition/interrupt/binding
arguments before the collector and handler. See the complete
[aggregate fetch example](../examples/aggregate-fetch.md).

## RowType Requirements

- Implement a **`builder` function** that returns a builder instance.
- Have a builder type that provides **setter methods** for all columns, either
  mutable or consuming.
- Implement **`build()`** to construct the final instance, returning either a
  row or a `Result`.
- For fallible builders, ensure that the builder error converts into the
  function’s error type `E`.

The `derive_builder::Builder` crate satisfies most of these requirements. You
only need to add a `builder()` method:

```rust
#[derive(derive_builder::Builder)]
struct RowType {
    //... fields go here.
}

impl RowType {
    fn builder() -> RowTypeBuilder
    {
        RowTypeBuilder::default()
    }
}
```

The default contract is mutable setters and a fallible build. Configure the two
characteristics independently in datasource metadata:

```toml
[package.metadata.database.default]
builder_setter = "consuming"
builder_build_fallible = false
```

The accepted setter values are `"mutable"` (the default) and `"consuming"`.
Mutable setters generate `builder.field(value);` after
`let mut builder = Row::builder();`. Consuming setters generate
`let builder = builder.field(value);` after `let builder = Row::builder();`,
allowing each setter to change the builder's type. The build option
independently selects `builder.build()?` (the default, `true`) or
`builder.build()` (`false`). All four combinations are supported.

Each SQL query function may override either option; omitted options retain the
selected datasource's values:

```rust
#[sql_fun::sql_query_one(
    sql = "select id, name from users",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn find_user(client: Client) -> Result<User, QueryError> {}
```

These options apply to `sql_query_one`, `sql_query_opt`, and `sql_query_many`,
including conditional SQL and interrupt paths. They do not affect `sql_statement`.
A typical `derive_builder` builder uses mutable/fallible behavior; a typical
`bon` builder uses consuming/infallible behavior. Generation follows the
configured contracts, without detecting either library. Setters must accept
the corresponding SQL column values; configure library-specific setter behavior
on your row type as needed.

## Error Type Requirements

The declared error must accept execution and row-extraction errors through
`From<tokio_postgres::Error>` or `From<rusqlite::Error>` for its connector.
A fallible row builder's error must also convert into it. Handler errors must
convert at the generated `?` boundary. `sql_statement` has no builder error.
There is no extra first-call prepare/column-mismatch check requiring
`From<std::io::Error>`.

[Interruption](query-interruption.md) adds connector- and macro-specific error
requirements, including SQLite's disconnection and cleanup callbacks.

## Collector Type

For an optional library that accepts ordered query rows and exposes immutable
row sets, see
[Collecting ordered rows with sql-fun-collector](../sql_fun_collector.md).

The `collector` is used with `sql_query_many` to accumulate rows. It is passed
to and returned from the `handler`; the generated function does not call any
methods on it directly.

A simple example is `VecCollector`, which collects rows into a `Vec<T>`:

```rust
struct VecCollector<T> {
    collected: Vec<T>,
}

impl<T> VecCollector<T> {
    pub fn new() -> Self {
        Self {
            collected: Vec::new(),
        }
    }

    pub async fn collect(
        mut self, value: T,
    ) -> Result<Self, std::convert::Infallible> {
        self.collected.push(value);
        Ok(self)
    }

    pub fn into_inner(mut self) -> Vec<T> {
        self.collected.shrink_to_fit();
        self.collected
    }
}
```

A query taking this collector has the following signature (the `UserRow`
builder follows the contract above):

```rust,ignore
#[sql_fun::sql_query_many(
    sql = "select id, name from users where name like ${name_like}"
)]
async fn select_users_by_name_like<F, Fut>(
    client: &tokio_postgres::Client,
    name_like: &str,
    mut collector: VecCollector<UserRow>,
    handler: F,
) -> Result<VecCollector<UserRow>, anyhow::Error>
where
    F: Fn(VecCollector<UserRow>, UserRow) -> Fut,
    Fut: std::future::Future<Output = Result<VecCollector<UserRow>, anyhow::Error>>,
{}
```

You can call it like this:

```rust
async fn execute_select_users_by_name_like(
    client: &tokio_postgres::Client,
    name_like: &str,
) -> Result<Vec<UserRow>, anyhow::Error> {
    let collector = VecCollector::new();

    let collector =
        select_users_by_name_like(
            client, name_like, collector, async move |collector, row| {
                Ok(collector.collect(row).await?)
            },
        )
        .await?;

    Ok(collector.into_inner())
}
```
