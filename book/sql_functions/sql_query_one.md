# `sql_query_one` function

`sql_query_one` executes an ad hoc SQL query that must return exactly one row.
The function returns `Result<Row, Error>`; zero rows or more than one row are errors.
Use `sql_query_opt` when a missing row is an expected result.

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C>(client: C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

Use `${parameter_name}` in the SQL to bind a function parameter's value to the
query. For example, `where id = ${id}` binds the value of the function's `id`
parameter.

This syntax works with both `sql` and `sql_file`. The placeholders are converted
to the connector's positional syntax: `$1`, `$2`, … for PostgreSQL and
`?1`, `?2`, … for SQLite.

## Attribute　arguments

| Attribute | Description |
| --- | --- |
| `datasource` | Optional datasource name from the metadata; defaults to `"default"`. |
| `sql` | SQL query embedded in the Rust source. Use either `sql` or `sql_file`. |
| `sql_file` | File containing the SQL query, optionally followed by a JSONPath selector. |
| `row_type` | Explicit result row type, when it cannot be inferred from the return type. |
| `error_type` | Explicit error type, when it cannot be inferred from the return type. |

### Inline SQL

Use `sql` to embed the query directly in the attribute.

For a PostgreSQL datasource:

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C>(client: C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

For a SQLite datasource named `edge` in the metadata:

```rust,ignore
#[sql_fun::sql_query_one(
    datasource = "edge",
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
fn find_user_by_id(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<UserRow, QueryError> {}
```

### SQL files

Use `sql_file` to load a query from a plain SQL file:

```rust,ignore
#[sql_fun::sql_query_one(sql_file = "select_user_by_id.sql")]
async fn find_user_by_id<C>(client: C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{}
```

Contents of `select_user_by_id.sql`:

```sql
select id, first_name, last_name from users where id = ${id}
```

Queries can also be stored in JSON, TOML, or YAML files. Append `#` and a JSONPath
that selects exactly one string, for example
`sql_file = "queries.toml#$.select_user_by_id.query"`.

Contents of `queries.toml`:

```toml
[select_user_by_id]
query = """
select id, first_name, last_name
from users
where id = ${id}
"""
```

## Function signature

関数シグニチャでは生成された関数のAPIを規定します。

関数名や引数、戻り値型すべてが関数を呼び出すアプリケーションコードに影響します。

第一引数はデータベース接続、利用するコネクタにおける接続を受け取ります。`sql-fun` は引数名や型を限定せず最初の引数が接続として機能するものとして関数呼び出しコードを生成します。

### Database connection

The first function argument supplies the connection used to execute the query.
This is required for both inline SQL and queries loaded with `sql_file`.

### PostgreSQL

`tokio-postgres` では引数の型により、DB接続とトランザクションのどちらを受け取るかもしくは両方で共通
に利用可能かを指定できます。

両方を受け入れる場合にはジェネリック引数を導入し、`tokio_postgres::GenericClient` を受け入れるようにします。

```rust
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}",
    row_type = UserRow,
    error_type = anyhow::Error
)]
async fn find_user_by_id<C>(client: &C, id: i64) -> QueryResult
where C: tokio_postgres::GenericClient + Sync,
{
}
```

トランザクション外の接続を受け入れる場合には、`tokio_postgres::Client` を受け入れるようにします。

```rust
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}",
    row_type = UserRow,
    error_type = anyhow::Error
)]
async fn find_user_by_id(client: &tokio_potgres::Client, id: i64) -> QueryResult
{
}
```

トランザクション配下の接続を受け入れる場合には、 `tokio_postgres::Transaction` を受け入れるようにします。

```rust
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}",
    row_type = UserRow,
    error_type = anyhow::Error
)]
async fn find_user_by_id(client: &tokio_potgres::Transaction, id: i64) -> QueryResult
{
}
```

注: `tokio_postgres::Transaction` の `client` 関数によりトランザクション対象の接続を得る事が可能なため、アプリケーションコード側でトランザクションから接続を得て関数を実行すればトランザクション外向けのSQL関数は実行可能です。このためSQL文をトランザクション配下で実行するべきか、トランザクション外のみで実行するべきかの厳密な限定にはなりません。

### SQLite

`rusqlite` では `&rusqlite::Transaction`　が `&rusqlite::Connection` に DeRef 可能なため `&rusqlite::Connection` 型で引数を定義した場合に通常の接続とトランザクション配下どちらでも利用可能になります。

引数を `&rusqlite::Transaction` で定義した場合、トランザクション配下でのみ呼び出し可能な関数になります。


### Return type inference

`sql-fun`　は関数シグニチャの戻り値の型から関数の返す正常時の行型、失敗時のエラー型を特定します。戻り値型の最初の型引数が行型、二番目の型引数がエラー型としての指定になります。戻り値型をエイリアスしている場合には属性のパラメータ `row_type`, `error_type` で行型およびエラー型を指定できます。 `row_type`, `error_type` を指定した場合には戻り値型からの取得はしません。

When not specified explicitly, `row_type` is taken from the return type's first
generic type argument and `error_type` from its second. The type's name and path
are ignored; the macro does not resolve type aliases.

| Return type | Inferred types |
| --- | --- |
| `Result<UserRow, QueryError>` | Both row and error types. |
| `std::result::Result<UserRow, QueryError>` | Both row and error types. |
| `anyhow::Result<UserRow>` | Row type only; specify `error_type = anyhow::Error`. |
| `QueryResult` (a type alias) | Neither; specify both attributes. |

Inference only extracts types; the declared return type must still be compatible
with the generated `Result` value.

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

エラー型はコネクタ実装のエラーや行型のビルダーが From により変換可能でなければならず、SQL関数への割り込みを利用する場合には関連関数の実装など追加要件があります。

## Interrupting a query

Interruption supplies an error value to return when a timeout or cancellation
occurs. Each connector uses a different interruption mechanism.

### PostgreSQL

The `#[interrupt_future]` argument is a callable with the bounds
`F: FnOnce() -> Fut` and `Fut: std::future::Future<Output = QueryError>`:

```rust,ignore
#[sql_fun::sql_query_one(
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
async fn find_user_by_id<C, F, Fut>(
    client: C,
    id: i64,
    #[interrupt_future] interrupt: &mut F,
) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = QueryError>,
{}
```

The generated function uses `tokio::select!` to race the query against
`interrupt()`. If the interrupt branch completes first, the function returns
`Err` with that value. Returning from the function does not guarantee that the
server has stopped executing the query.

> **Note:** Interruption stops the client-side operation; it does not cancel
> processing on the server. To stop server-side processing, the application must
> call `tokio_postgres::CancelToken::cancel_query` or close the server connection
> so that PostgreSQL terminates the associated backend process.
>
> `tokio_postgres::CancelToken::cancel_query`は追加の接続を利用し呼び出しには TLS 設定パラメータも必要となります。タイムアウト等、DBが過負荷な場合には追加接続を受け入れられない可能性もあるため複雑化しやすく困難な実装となる可能性があります。
>
> When using deadpool for connection management, `drop(conn.take());` can be used
> to disconnect and discard the connection. With bb8, the current API provides no
> way to discard a connection without returning it to the pool.
>
> Connection management is the responsibility of the application using `sql-fun`.
> This includes cancelling server-side processing after an interruption and
> preventing reuse of any connection that has become invalid as a result.

### SQLite

The `#[interrupt_channel]` argument `Arc<Mutex<Receiver<QueryError>>>` passed by value:

```rust,ignore
#[sql_fun::sql_query_one(
    datasource = "sqlite",
    sql = "select id, first_name, last_name from users where id = ${id}"
)]
fn find_user_by_id(
    connection: &rusqlite::Connection,
    id: i64,
    #[interrupt_channel]
    interrupt: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<QueryError>>>,
) -> Result<UserRow, QueryError> {}
```

The generated function installs a `progress_handler` that polls the channel
with `try_recv()` every 1,000 SQLite virtual machine instructions. An empty channel
allows execution to continue. A received error is stored and the handler requests
interruption; a disconnected channel also requests interruption.

The application must enable rusqlite's `hooks` feature to use `progress_handler`.

After execution, the handler is removed. If cleanup succeeds, a stored interrupt
error takes precedence over the query result.

> **Note:** If the interrupt channel closes unexpectedly,
> `interrupt_disconnected()` is called as an associated function on the error
> type, with no arguments. It must return an error value (`Self`).
>
> If removing the SQLite interrupt handler fails,
> `interrupt_handler_cleanup_error` is called as an associated function on the
> error type. It takes two arguments: the `rusqlite::Error` from the cleanup
> failure and a `Result<T, Self>` containing the query result already obtained.
> It must return a `Result<T, Self>`.
>
> To continue processing with the original query result despite the cleanup
> failure, return the second argument unchanged. This preserves both successful
> results and query errors.
