# SQL Functions

`sql-fun` code generators translates `SQL` into Rust function.

## Attribute arguments

Attribute arguments specifies SQL statement and datasource.

- `sql`: SQL statement literal, conflicts with `sql_file`
- `sql_file`: SQL statement from file, conflicts with `sql`
  - SQL statement embedding in structured file
    - When `sql_file` contains `#`, file can be JSON/TOML/YAML formatted structured file.
    - After `#` for JSON-Path to SQL string.
  - relative path from source file.
- `datasource`: specify datasource name

### Attribute argument examples

Specify SQL statement in source file. `sql` argument or `sql_file` argument.

```rust
#[sql_fun::sql_query_many( sql="select id, name from users" )]
```

Specify SQL statement by separated file.

```rust
#[sel_fun::sql_query_one( sql_file = "select_user_by_id.sql" )]
```

```sql:select_user_by_id.sql
select id, name from users where id = ${id}
```

Specify SQL statement by TOML.

```rust
#[sql_fun::sql_query_one( sql_file = "queries.toml#$.select_user_by_id" )]
```

```toml:queries.toml
select_user_by_id = """
  select id, name from users where id=${id}
"""
```

`datasource` specify a database schema.

```toml:Cargo.toml
[package.metadata.database]
default = {
　　engine="postgres",
  connector="tokio_postgres"
}
edge = {
  engine="sqlite"
  connector="rusqlite"
}
```

```rust
// query from default
#[sql_fun::sql_query_many( sql="select id, name from users" )]
...

// query from edge
#[sql_fun::sql_query_many( 
    datasource="edge",
    sql="select id, name from products" )]
...
```


## Function signature requirements

- Functions must be annotated with one of the following:
  - `sql_fun::sql_query_one`: Expects a single row; return type must be `Result<RowType, E>`.
  - `sql_fun::sql_query_opt`: Expects zero or one row; return type must be `Result<Option<RowType>, E>`.
  - `sql_fun::sql_query_many`: Expects multiple rows (possibly zero); return type must be `Result<Collector, E>`.
  - `sql_fun::sql_statement`: Executes a statement that returns no rows; return type must use the connector's affected-row count type (`u64` for `tokio-postgres`, `usize` for `rusqlite`).
- When async connector is used, function requires `async`.
  - `tokio-postgres` is a async connector , `rusqlite` is not a async connector.
- The first argument represents the database connection.
  - This may be any type, such as `Client`, `Transaction`, or a user-defined wrapper type.
    - It must support the methods `prepare`, `query_one`, `query_opt`, `query_raw`, or `execute` with compatible signatures.
- A parameter with `#[interrupt_future]` or `#[interrupt_channel]` is used to interrupt query execution.
  - `#[interrupt_future]` combined for async connector.
    - When future completed, stop query execution.
  - `#[interrupt_channel]` is combined with a sync connector.
    - `rusqlite` uses `Connection::progress_handler`; a received message requests query interruption.

- All other parameters are used for SQL parameter binding.

## Additional Requirements for sql_query_many
  - The last two parameters must be the `collector` and the `handler`, in that order.
    - Their names do not matter; only their position is used.
    - The `handler` is called for each row in the result set. It consumes the `collector` and returns an updated one.
    - The `collector` holds query execution state:
      - It must be declared `mut`.
      - Any type may be used; `sql_query_many` does not directly call it.
      - The generated function only owns and passes it along.
      - The final value is returned by the function.
  - The `handler` must be a generic `Fn` or closure.
    - `R` is the row type, inferred from the handler's parameter.
    - With `tokio-postgres`, its form is `Fn(C, R) -> Fut`, where `Fut` is an async future returning `Result<C, E>`.
    - With `rusqlite`, its form is `Fn(C, R) -> Result<C, E>`.


## RowType Requirements

- Implement a **`builder` function** that returns a builder instance.
- Have a builder type that provides **setter methods** for all columns.
- Implement **`build()`** to construct the final instance.
- Ensure that the builder type’s error type is compatible with the function’s return error type `E`.

The `derive_builder::Builder` crate satisfies most of these requirements. You only need to add a `builder()` method:


```rust
#[derive(derive_builder::Builder)]
struct RowType {
    //... fields gose here.
}

impl RowType {
    fn builder() -> RowTypeBuilder
    {
        RowTypeBuilder::default()
    }
}
```

## Error Type Requirements

The Function returns `Result<IdRow, E>`, where `E` must:

- Implement `From<tokio_postgres::Error>` (for query execution failures).
- Implement `From<std::io::Error>` (for unexpected column mismatches in the prepare check statement).
- Be capable of handling any errors returned by the builder type.


## Collector Type

For an optional library that accepts ordered query rows and exposes immutable row sets,
see [Collecting ordered rows with sql-fun-collector](./sql_fun_collector.md).

The `collector` is used with `sql_query_many` to accumulate rows. It is passed to and returned from the `handler`; the generated function does not call any methods on it directly.

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

You can use it like this:

```rust
async fn execute_select_users_by_name_like(
    client: &Client,
    name_like: &str,
) -> Result<Vec<RowType>, anyhow::Error> {
    let collector = VecCollector::new();

    let collector =
        select_users_by_name_like(client, name_like, collector, async move |collector, row| {
            Ok(collector.collect(row).await?)
        })
        .await?;

    Ok(collector.into_inner())
}
```


## Parameter binding

- In the SQL string, bind parameters using `${...}` syntax.
- Named parameters like `${name}` are converted to positional placeholders like `$1`, `$2`, etc.

## Conditional SQL

- Write conditional SQL fragments using `#{...}` syntax.
- Use the `#[sql_fragment(...)]` attribute to define their expansion.

The attribute takes three parameters:

 1. The name of the fragment.
 1. The match argument.
 1. The match arms.

### Match Arms

The third argument to #[sql_fragment(...)] is a Rust-style match block that defines how to expand the fragment based on the input values.

Each arm should returns:
 - literal string representing the SQL fragment to inject.
 - the `INVALID_QUERY` causes a runtime error

For example:

```rust
#[sql_fun::sql_query_many(sql = "select id, name from users #{where_clause}")]
#[sql_fragment(
    where_clause,
    name,
    {
        Some(n) => "where name=${n}",
        None => ""
    }
)]
```

This means:

 - If `name` is `Some(n)`, the SQL becomes: `select ... where name=$1`
 - If `name` is `None`, the fragment is empty, so no WHERE clause is added

Each match arm is expanded into a separate internal function (see below), so naming is important.

### The `#[cond]` Attribute

Parameters marked with `#[cond]` are *used only for fragment expansion* and are *not passed* to the internal function generated for each match arm.

This attribute tells `sql-fun` that the parameter is *consumed in the match expression* defined in `#[sql_fragment(...)]`. Since its value is already matched and handled in the outer function, it does not need to be passed down to the branch functions.

Without `#[cond]`, all parameters are assumed to be forwarded to the generated internal function.

### Internal Function Naming

The function name is derived by combining the original function name with the match arm's pattern:

Match arms with `_` are expanded as function names ending in `_`.

```
Original function name: find_user

Match:         Some(n)
Generated fn:  find_user_some_n

Match:         (None, Some(age_min), Some(age_max))
Generated fn:  find_user_none_some_age_min_some_age_max

Match:         _
Generated fn:  find_user_
```

These names also appear as query identifiers in the generated `sql-fun.{schema}.toml`.

    💡 Use meaningful pattern variable names to keep generated function and query names readable.


### Conditional SQL Example:

```rust
#[sql_fun::sql_query_many(sql = "select id, name from users #{where_clause}")]
#[sql_fragment(
    where_clause,
    name,
    {
        Some(n) => "where name=${n}",
        None => ""
    }
)]
async fn find_user(
    client: &Client,
    #[cond] name: Option<&str>,
    mut user_collector: VecCollector<UserRow>,
    handler: F,
) -> Result<VecCollector<UserRow>, anyhow::Error>
where
    F: Fn(VecCollector<UserRow>, UserRow) -> Fut,
    Fut: Future<Output = Result<VecCollector<UserRow>, anyhow::Error>>,
{}
```

It just expands into bellow.

```rust
async fn find_user(...) -> Result<...> {
    match name {
        Some(n) => find_user_some_n(client, n, collector, handler),
        None => find_user_none(client, collector, handler),
    }

    #[sql_fun::sql_query_many(sql = "select id, name from users where name=${n}")]
    async fn find_user_some_n(...) -> Result<...> {}

    #[sql_fun::sql_query_many(sql = "select id, name from users")]
    async fn find_user_none(...) -> Result<...> {}
}
```

This expansion is recursive — each case is its own `sql_query_many`.


You can also match on a tuple:

```rust
#[sql_fun::sql_query_many( sql= "select id, name from users #{where_clause}")]
#[sql_fragment(
    where_clause,
    (name, age_min, age_max),
    {
        (Some(n), Some(age_min), Some(age_max)) => "where name=${n} and age between ${age_min} and ${age_max}",
        (Some(n), None, None) => "where name=${n}",
        (None, Some(age_min), Some(age_max)) => "where age between ${age_min} and ${age_max}",
        (None, None, None) => "",
        _ => INVALID_QUERY
    }
)]
async fn find_user(...) -> Result<...> {}

```

The special value `INVALID_QUERY` triggers a runtime error with `std::io::ErrorKind::InvalidData`.
This indicates that no matching SQL clause is available for the input.

You can also avoid `INVALID_QUERY` by passing more strongly-typed inputs, like `Option<(T1, T2)>`.

You choose:
- Use stricter types and guarantee compile-time safety
- Or allow looser input and handle invalid combinations at runtime


## Aggregate entity fetch example for sql-fun

### Example: Fetching an aggregate with multiple child tables

In a typical application, a “parent” entity often has several child
tables. For example:

```sql
create table parent (
    id          bigint primary key,
    col1        text not null,
    col2        text not null,
    status      text not null,
    created_at  timestamptz not null
);

create table child1 (
    id          bigint primary key,
    parent_id   bigint not null references parent(id),
    value1      text not null
);

create table child2 (
    id          bigint primary key,
    parent_id   bigint not null references parent(id),
    value2      integer not null
);
```

We want to fetch an aggregate:

```rust
pub struct Aggregate {
    pub parent: ParentRow,
    pub child1: Vec<Child1Row>,
    pub child2: Vec<Child2Row>,
}
```

in a way that:

- does **not** repeat the full parent row for every child row, and
- keeps the selection and ordering of the parent consistent across all
  child queries.

`sql-fun` supports this pattern with:

1. one root query for the parent,
2. child queries that import the root query as a CTE, and
3. a collector that merges all rows into a final aggregate.

#### Row types

```rust
use tokio_postgres::Client;
use std::future::Future;

#[derive(derive_builder::Builder, Clone)]
pub struct ParentRow {
    pub id: i64,
    pub col1: String,
    pub col2: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ParentRow {
    pub fn builder() -> ParentRowBuilder {
        ParentRowBuilder::default()
    }
}

#[derive(derive_builder::Builder, Clone)]
pub struct Child1Row {
    pub id: i64,
    pub parent_id: i64,
    pub value1: String,
}

impl Child1Row {
    pub fn builder() -> Child1RowBuilder {
        Child1RowBuilder::default()
    }
}

#[derive(derive_builder::Builder, Clone)]
pub struct Child2Row {
    pub id: i64,
    pub parent_id: i64,
    pub value2: i32,
}

impl Child2Row {
    pub fn builder() -> Child2RowBuilder {
        Child2RowBuilder::default()
    }
}

pub struct Aggregate {
    pub parent: ParentRow,
    pub child1: Vec<Child1Row>,
    pub child2: Vec<Child2Row>,
}
```

#### Root query: select the parent set once

We define the root “parent set” as a normal `sql_query_many` function:

```rust
#[sql_fun::sql_query_many(
    r#"
        select
            a.id,
            a.col1,
            a.col2,
            a.status,
            a.created_at
        from parent a
        where a.status     = ${status}
          and a.created_at >= ${since}
        order by a.id
    "#
)]
async fn select_parent_by_status_since<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    collector: AggregateCollector,
    handler: F,
) -> Result<AggregateCollector, anyhow::Error>
where
    F: Fn(AggregateCollector, ParentRow) -> Fut,
    Fut: Future<Output = Result<AggregateCollector, anyhow::Error>>,
{
}
```

This function defines the *meaning* of “the set of parents” exactly once.

#### Child queries: reuse the root selection as a CTE

Each child query imports the parent selection as a CTE via `#[sql_fun::with]`.
The entire SQL of the root query is pulled in as a CTE, so the `WHERE`
and `ORDER BY` clauses are always identical.

```rust
#[sql_fun::sql_query_many(
    r#"
        select
            b.id,
            b.parent_id,
            b.value1
        from selected_parent a
        inner join child1 b on a.id = b.parent_id
        order by a.id, b.id
    "#
)]
#[sql_fun::with(selected_parent = select_parent_by_status_since)]
async fn select_child1<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    collector: AggregateCollector,
    handler: F,
) -> Result<AggregateCollector, anyhow::Error>
where
    F: Fn(AggregateCollector, Child1Row) -> Fut,
    Fut: Future<Output = Result<AggregateCollector, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(
    r#"
        select
            c.id,
            c.parent_id,
            c.value2
        from selected_parent a
        inner join child2 c on a.id = c.parent_id
        order by a.id, c.id
    "#
)]
#[sql_fun::with(selected_parent = select_parent_by_status_since)]
async fn select_child2<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    collector: AggregateCollector,
    handler: F,
) -> Result<AggregateCollector, anyhow::Error>
where
    F: Fn(AggregateCollector, Child2Row) -> Fut,
    Fut: Future<Output = Result<AggregateCollector, anyhow::Error>>,
{
}
```

Conceptually, each child query is expanded to:

```sql
with selected_parent as (
    -- the SQL attached to `select_parent_by_status_since`
    select ...
    from parent a
    where ...
    order by a.id
)
select ...
from selected_parent a
join childX ...
order by a.id, child_id;
```

The root selection exists in only one place, but all child queries share
its definition by construction.

#### Collector: accumulate rows and merge at the end

`AggregateCollector` receives rows from all three queries and merges
them into a `Vec<Aggregate>`.

```rust
pub struct AggregateCollector {
    parents: Vec<ParentRow>,
    child1: Vec<Child1Row>,
    child2: Vec<Child2Row>,
}

impl AggregateCollector {
    pub fn new() -> Self {
        Self {
            parents: Vec::new(),
            child1: Vec::new(),
            child2: Vec::new(),
        }
    }

    pub async fn push_parent(
        mut self,
        row: ParentRow,
    ) -> Result<Self, std::convert::Infallible> {
        self.parents.push(row);
        Ok(self)
    }

    pub async fn push_child1(
        mut self,
        row: Child1Row,
    ) -> Result<Self, std::convert::Infallible> {
        self.child1.push(row);
        Ok(self)
    }

    pub async fn push_child2(
        mut self,
        row: Child2Row,
    ) -> Result<Self, std::convert::Infallible> {
        self.child2.push(row);
        Ok(self)
    }

    pub fn finish(self) -> Vec<Aggregate> {
        // parents is ordered by id
        // child1  is ordered by parent_id, id
        // child2  is ordered by parent_id, id

        fn merge_children<P, C, K, FParentKey, FChildKey, FSlot>(
            parents: &mut [P],
            children: &[C],
            parent_key: FParentKey,
            child_parent_key: FChildKey,
            mut children_slot: FSlot,
        )
        where
            K: Ord + Eq,
            FParentKey: Fn(&P) -> &K,
            FChildKey: Fn(&C) -> &K,
            FSlot: Fn(&mut P) -> &mut Vec<C>,
            C: Clone,
        {
            let mut i = 0;
            let mut j = 0;

            while i < parents.len() && j < children.len() {
                use std::cmp::Ordering::*;
                match parent_key(&parents[i]).cmp(child_parent_key(&children[j])) {
                    Less => i += 1,
                    Greater => j += 1,
                    Equal => {
                        let slot = children_slot(&mut parents[i]);
                        while j < children.len()
                            && parent_key(&parents[i]) == child_parent_key(&children[j])
                        {
                            slot.push(children[j].clone());
                            j += 1;
                        }
                        i += 1;
                    }
                }
            }
        }

        let mut aggregates: Vec<Aggregate> = self
            .parents
            .into_iter()
            .map(|p| Aggregate {
                parent: p,
                child1: Vec::new(),
                child2: Vec::new(),
            })
            .collect();

        merge_children::<Aggregate, Child1Row, i64, _, _, _>(
            &mut aggregates,
            &self.child1,
            |a| &a.parent.id,
            |c| &c.parent_id,
            |a| &mut a.child1,
        );

        merge_children::<Aggregate, Child2Row, i64, _, _, _>(
            &mut aggregates,
            &self.child2,
            |a| &a.parent.id,
            |c| &c.parent_id,
            |a| &mut a.child2,
        );

        aggregates
    }
}
```

#### Putting it together

The public API becomes a single “fetch aggregate” function:

```rust
pub async fn fetch_aggregate_by_status_since(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Aggregate>, anyhow::Error> {
    let collector = AggregateCollector::new();

    let collector = select_parent_by_status_since(
        client,
        status,
        since,
        collector,
        |collector, row| async move { Ok(collector.push_parent(row).await?) },
    )
    .await?;

    let collector = select_child1(
        client,
        status,
        since,
        collector,
        |collector, row| async move { Ok(collector.push_child1(row).await?) },
    )
    .await?;

    let collector = select_child2(
        client,
        status,
        since,
        collector,
        |collector, row| async move { Ok(collector.push_child2(row).await?) },
    )
    .await?;

    Ok(collector.finish())
}
```

`sql-fun` focuses strictly on the “connection layer” between Rust and SQL.
It parses SQL, binds it to Rust function signatures, and lets one query
reuse another as a CTE in a safe, structured way. Everything that happens
*inside* a result set — aggregation, object mapping, domain modeling —
is intentionally left to user code or external libraries.
