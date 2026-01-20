# Batch query support

Batch queries are multiple, semicolon-separated queries executed with a few round trips, based on the `PostgreSQL simple query protocol`.
Most application queries run with the `PostgreSQL extended query protocol`.

## Comparison with `tokio-postgres` pipeline

`tokio-postgres` supports pipeline execution.

```rust
let f1 = client.query("SELECT ...", &[]);
let f2 = client.execute("INSERT ...", &[]);
let (rows, n) = tokio::try_join!(f1, f2)?;
```

When you create futures without awaiting them, the driver keeps queries in flight.

- If you want centralized error handling for multiple in-flight queries, choose `sql-fun` batch query support: the generated code handles rollback/drop and result dispatch consistently.
- If you only need to reduce RTT without rewriting SQL, pipeline mode in `tokio-postgres` is sufficient and keeps extended-protocol semantics.
- Pipeline mode sends multiple queries without waiting, but error handling remains application-owned per in-flight query.
- `sql-fun` batch rewrites SQL to support parameter binding over the simple query protocol using a temporary parameter table.

## `PostgreSQL simple query protocol` advantages and disadvantages

- Advantages
  - One or a few round trips to execute multiple queries.
  - No latency between queries; executes the next query immediately.
- Disadvantages
  - No parameter binding support.
  - Poor result set decoding support.

## `sql-fun` batch query rewriting support basics

`sql-fun` batch query rewriter enables parameter binding with a temporary parameter table.

```sql
select id, name from users where id=${user_id};
select user_id, product_id, amount from order_lines where user_id=${user_id};
```

These queries are rewritten into three round-trip queries.

First query runs with the extended query protocol.

```sql
create temporary table batch_support_table
as
  select $1::bigint as user_id
```

Second query runs with the simple query protocol.

```sql
select id, name from users where id=(select user_id from batch_support_table);
select user_id, product_id, amount from order_lines where user_id=(select user_id from batch_support_table);
drop batch_support_table;
```

Third query is used when batch query execution fails.

```sql
drop batch_support_table;
```

### Actual temporary table used by `sql-fun`

- The temporary table name is prefixed with `params_` and uses a SHA256 hash of the original query text.
- Column names for parameters
  - When a parameter is described as PostgreSQL placeholder syntax (e.g., `$1`) then `param1`.
  - When a parameter is described as `sql-fun` named parameter syntax (e.g., `${user_id}`) then the parameter name.
- Dropping temporary tables
  - `sql_fun::batch` determines the transaction scope by the `client` parameter type.
  - When `Transaction` is used, the batch runs under a transaction.
    - When an error occurs in the batch, a rollback revokes `create temporary table`.
  - When `Client` is used, the batch runs as auto-commit.
    - When an error occurs in the batch, generated code executes `drop` on the temporary table.

## Receiving results

`sql_fun::batch` generates query execution and handles the whole response as a stream, and `sql_fun::batch_member_*` handles a single query response stream.

`sql_fun::batch`'s `handlers` attribute specifies a sequence of result set handlers.

- `sql_fun::batch_member_many` handles multi-row results
- `sql_fun::batch_member_one` handles single-row results
- `sql_fun::batch_member_exec` handles no rows, only affected row count

`sql_fun::batch_member_many` follows the `sql-fun` concept: a `collector` and `handler` pair handles one result set.

```rust
#[sql_fun::batch(r#"
    select id, name from users where id=${user_id} ;
    select user_id, product_id, amount from order_lines where user_id=${user_id} ;
"#),
    // The first result set is passed to users_handler,
    // and the second result set is passed to order_lines_handler.
    handlers = ( users_handler, order_lines_handler )
]
async fn select_user_name_and_order_lines<First, FirstFut, Second, SecondFut>(
    client: &mut Client,
    collectors: (UsersCollector, OrderLineCollector),
    handlers: (First, Second),
) -> Result<(UsersCollector, OrderLineCollector), QueryError>
where
    First: Fn(UsersCollector, UsersRow) -> FirstFut,
    FirstFut: Future<Output = Result<UsersCollector, QueryError>>,
    Second: Fn(OrderLineCollector, OrderLineRow) -> SecondFut,
    SecondFut: Future<Output = Result<OrderLineCollector, QueryError>>,
{
    // `sql-fun` generates the batch execution code here.
}

#[sql_fun::batch_member_many()]
async fn users_handler<F, Fut>(
    stream: SimpleQueryStream,
    collector: UsersCollector,
    handler: F
) -> Result<UsersCollector, QueryError>
where
    F: Fn(UsersCollector, UsersRow) -> Fut,
    Fut: Future<Output = Result<UsersCollector, QueryError>>,
{
    // `sql-fun` generates the stream handler here.
}

#[sql_fun::batch_member_many()]
async fn order_lines_handler<F, Fut>(
    stream: SimpleQueryStream,
    collector: OrderLineCollector,
    handler: F
) -> Result<OrderLineCollector, QueryError>
where
    F: Fn(OrderLineCollector, OrderLineRow) -> Fut,
    Fut: Future<Output = Result<OrderLineCollector, QueryError>>,
{
    // `sql-fun` generates the stream handler here.
}

```
