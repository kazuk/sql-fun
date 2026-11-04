# Query interruption

Interruption returns an application error when a supplied future or channel
requests it. It applies to `sql_query_one`, `sql_query_opt`, `sql_query_many`
and `sql_statement`, with different connector contracts. Put the interrupt
argument before the collector/handler pair in `sql_query_many`.

## PostgreSQL

For the three query macros, `#[interrupt_future]` marks a mutable reference to a
future whose output is the function's declared error type. It is a future value,
not a callable that creates a future:

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C>(
    client: &C,
    id: i64,
    #[interrupt_future]
    interrupt: &mut (impl std::future::Future<Output = QueryError> + Unpin),
) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

A pinned mutable future can be used when its underlying future is not `Unpin`.
The generated `tokio::select!` polls the query and interrupt together. If the
interrupt wins, its output is returned as `Err` without error conversion.
For `sql_query_many`, this happens while starting the query and waiting for
each next row; the interrupt is not polled while awaiting the row handler.

`sql_statement` awaits its supplied future directly and converts its output
into the function error using `Into`. For example:

```rust,ignore
#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn delete_user<F>(
    client: &tokio_postgres::Client,
    id: i64,
    #[interrupt_future] interrupt: F,
) -> Result<u64, anyhow::Error>
where
    F: std::future::Future<Output = anyhow::Error>,
{}
```

Application dependencies must provide the generated connector and async calls,
including `tokio::select!` and, for multi-row queries, `futures_util::TryStreamExt`.

### Server-side cancellation

Returning an interrupt error does not guarantee that PostgreSQL has stopped
executing the query. Connection management and server-side cancellation belong
to the application. Use the driver's `CancelToken::cancel_query` with the
appropriate TLS configuration when required; cancellation uses a separate
connection and its request is not a guarantee of successful cancellation.
Discard connections that application policy considers unusable rather than
returning them to a pool. Consult the pool's API for how to discard a connection;
dropping a pooled guard may only return it to the pool. Simply dropping a client
handle is not a general guarantee of backend termination.

## SQLite

`#[interrupt_channel]` receives an
`Arc<Mutex<std::sync::mpsc::Receiver<QueryError>>>` by value. The error value must
be `Send + 'static` for the installed handler. The connection must
support mutable `progress_handler` calls; with rusqlite 0.41 this means a mutable
connection and the `hooks` feature:

```rust,ignore
#[sql_fun::sql_query_one(
    datasource = "edge",
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
fn find_user_by_id(
    connection: &mut rusqlite::Connection,
    id: i64,
    #[interrupt_channel]
    interrupt: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<QueryError>>>,
) -> Result<UserRow, QueryError> {}
```

The generated function installs a progress handler that polls `try_recv()`
every 1,000 SQLite virtual machine instructions. An empty channel permits
execution to continue. A received error is stored and requests interruption;
a disconnected channel stores `QueryError::interrupt_disconnected()` and also
requests interruption. This is cooperative SQLite execution interruption, not
an independent task that can preempt arbitrary Rust row-handler work.

The error type needs these associated functions in addition to its driver and
builder conversions:

```rust,ignore
impl QueryError {
    fn interrupt_disconnected() -> Self { /* application error */ }

    fn interrupt_handler_cleanup_error<T>(
        cleanup_error: rusqlite::Error,
        query_result: Result<T, Self>,
    ) -> Result<T, Self> { /* application policy */ }
}
```

After execution the handler is removed. If removal succeeds, a stored interrupt
error takes precedence over the query result. If removal fails,
`interrupt_handler_cleanup_error` receives that failure and the query result
already obtained. Returning its second argument unchanged preserves either a
successful query result or its original query error. The connection's progress
handler is replaced and then removed; the macro does not restore an earlier
handler. A failed handler installation propagates as a driver error.
