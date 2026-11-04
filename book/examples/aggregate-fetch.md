# Aggregate Fetch without sql-fun-collector

This PostgreSQL example uses a hand-written collector. The `sql-fun-collector`
component is optional; this approach is supported and is not deprecated. A
function-level collector is simply the value passed through each row handler,
not a requirement to use that component.

## Fetching an aggregate with multiple child tables

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

We want an `Aggregate` containing the parent row and vectors of both child row
types (defined below), in a way that:

- does **not** repeat the full parent row for every child row, and
- keeps the selection and ordering of the parent consistent across all
  child queries.

`sql-fun` supports this pattern with:

1. one root query for the parent,
2. root and child queries that reference the same CTE Catalog definition, and
3. a collector that merges all rows into a final aggregate.

## Row types

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

## Root query: select the parent set once

Install the following definition in the directory explicitly configured as the
selected datasource's `cte_catalog` (for example `cte-catalog/parents.toml`).
See [CTE Catalog](../reusable_cte.md) for the complete format and dependency
rules. The name below identifies a SQL catalog entry, not a Rust function:

```toml
sql_fun_cte_catalog_format_version = "1"
sql_dialect = "PostgreSQL"

[cte.selected_parent]
query = """
select a.id, a.col1, a.col2, a.status, a.created_at
from parent a
where a.status = ${status} and a.created_at >= ${since}
order by a.id
"""
```

The root query uses the catalog entry as `selected_parent`:

```rust
#[sql_fun::sql_query_many(
    sql = "select id, col1, col2, status, created_at
           from selected_parent order by id"
)]
#[with(cte_name = "selected_parent", alias = "selected_parent")]
async fn select_parent_by_status_since<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    mut collector: AggregateCollector,
    handler: F,
) -> Result<AggregateCollector, anyhow::Error>
where
    F: Fn(AggregateCollector, ParentRow) -> Fut,
    Fut: Future<Output = Result<AggregateCollector, anyhow::Error>>,
{
}
```

The catalog definition supplies the parent selection to all three queries.

## Child queries: reuse the root selection as a CTE

Each child references the same SQL catalog entry through the macro helper
`#[with(cte_name = "selected_parent", alias = "selected_parent")]`. The parent
selection criteria come from the catalog; each outer query explicitly orders
its own results for the merge below.

```rust
#[sql_fun::sql_query_many(
    sql = r#"
        select
            b.id,
            b.parent_id,
            b.value1
        from selected_parent a
        inner join child1 b on a.id = b.parent_id
        order by a.id, b.id
    "#
)]
#[with(cte_name = "selected_parent", alias = "selected_parent")]
async fn select_child1<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    mut collector: AggregateCollector,
    handler: F,
) -> Result<AggregateCollector, anyhow::Error>
where
    F: Fn(AggregateCollector, Child1Row) -> Fut,
    Fut: Future<Output = Result<AggregateCollector, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(
    sql = r#"
        select
            c.id,
            c.parent_id,
            c.value2
        from selected_parent a
        inner join child2 c on a.id = c.parent_id
        order by a.id, c.id
    "#
)]
#[with(cte_name = "selected_parent", alias = "selected_parent")]
async fn select_child2<F, Fut>(
    client: &Client,
    status: &str,
    since: chrono::DateTime<chrono::Utc>,
    mut collector: AggregateCollector,
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
    -- the SQL from the selected_parent catalog entry
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
its catalog definition. This shares SQL text, not a materialized result across
queries. If a consistent database snapshot is required, run the queries under
an appropriate transaction/isolation policy; the catalog does not provide one.

## Collector: accumulate rows and merge at the end

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
            children_slot: FSlot,
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
                match parent_key(&parents[i])
                    .cmp(child_parent_key(&children[j]))
                {
                    Less => i += 1,
                    Greater => j += 1,
                    Equal => {
                        while j < children.len()
                            && parent_key(&parents[i])
                                == child_parent_key(&children[j])
                        {
                            children_slot(&mut parents[i])
                                .push(children[j].clone());
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

## Putting it together

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

The example requires the application dependencies `tokio-postgres` (with
`with-chrono-0_4` for `timestamptz`), `tokio`, `futures-util`, `chrono`,
`derive_builder` and `anyhow`, plus the `sql-fun` macros and prepared datasource
metadata/lockfiles. See [installation](../installation.md).

`sql-fun` binds SQL to Rust function signatures; the Catalog shares SQL
selection definitions. Aggregation, object mapping and domain modeling remain
application code. Use the same approach with your own collector, or choose the
optional [sql-fun-collector](../sql_fun_collector.md) component when its ordered
row-set API fits your application.
