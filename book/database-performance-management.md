# Database Performance Management

The content described in this section is not a core feature of `sql-fun` itself;
in fact, `sql-fun` does not provide any database management functionality.
Instead, `sql-fun` generates a TOML file (by default named
`sql-fun.develop.toml` and placed alongside `Cargo.toml`) containing extracted
SQL statements. By applying the `EXPLAIN` statement to the SQL queries in this
file, you can obtain execution plans and cost estimates from the Postgres server
for all queries used in your application.

Of course, to obtain accurate cost estimates, it is necessary to have a database
with a data volume and appropriate data distribution equivalent to that of the
production DB server.

Since a database connection is required, and it is necessary to connect to a
database containing a substantial amount of data, it is recommended to perform
these operations on a CI server.

## Query Performance Management

In this document, we clearly distinguish between the terms "database performance
management" and "query performance management."

"Database performance management" refers to the comprehensive optimization of
the entire database in a real-world production environment. This includes
considering overall system statistics such as query execution counts, access
patterns, and resource consumption.

On the other hand, "query performance management" focuses on the analysis and
improvement of execution plans and cost estimates for individual SQL queries. In
this context, we do not take into account how many times each query is actually
executed in real use cases. As a result, even queries that are rarely executed
in practice are included in the analysis. However, the advantage of this
approach is that you can start performance management without needing to collect
statistics such as execution counts. This makes it an ideal first step in
performance management.

In summary, it is recommended to begin with query-level performance analysis
and, as needed, progress to database-wide optimization that incorporates
operational statistics from the production environment.

### Tools for Query Extraction

The first tool to introduce is [jyt](https://github.com/ken-matsui/jyt). This
utility allows you to convert files between JSON, YAML, and TOML formats. To
convert a TOML file to JSON, you can use the `tj` option.

By combining `jyt` with `jq`, you can extract all Rust function names and their
corresponding queries from the TOML file efficiently. For example, you can first
convert the TOML file to JSON using `jyt tj`, and then use `jq` to filter and
display the relevant fields.

This approach enables you to systematically analyze and manage the queries
defined in your project.

Example: extracting queries in your application.

```bash
jyt tj sql-fun.develop.toml \
 | jq '[.queries | to_entries[] | {name: .key, sql: .value.sql}]'
```

### Using the `EXPLAIN` SQL Command and the `GENERIC_PLAN` Parameter

PostgreSQL Documentation:
[EXPLAIN](https://www.postgresql.org/docs/current/sql-explain.html)

The next step is to use the `EXPLAIN` SQL command. In PostgreSQL, the
`GENERIC_PLAN` parameter can be utilized to obtain the worst-case execution plan
for a query. While PostgreSQL may actually use a better plan in practice, if the
optimizer determines that the benefit of a custom plan is limited, the
`GENERIC_PLAN` will ultimately be chosen for query execution.

One notable advantage of `GENERIC_PLAN` is that you can run `EXPLAIN` without
providing actual parameter placeholder values. This makes it possible to analyze
the execution plan for parameterized queries without needing to supply concrete
values for each parameter.

By leveraging `GENERIC_PLAN`, you can efficiently evaluate the baseline
performance characteristics of your queries, even in the absence of real
parameter data.

Example: Explain all queries in your application

> This example assumes that you have already set the necessary PostgreSQL client
> environment variables (such as PGHOST, PGPORT, PGUSER, PGDATABASE, etc.), so
> that you can connect to your database using the psql command without
> specifying additional connection options.

```bash
#!/bin/bash

for line in $(jyt tj sql-fun.develop.toml \
 | jq -c '.queries | to_entries[] | {name: .key, sql: .value.sql}')
do
  NAME=$(echo ${line} | jq -r '.name')
  SQL=$(echo ${line} | jq -r '.sql')
  EXPLAIN_SQL="EXPLAIN ( GENERIC_PLAN FORMAT JSON ) ${SQL}"

  psql -c "${EXPLAIN_SQL}" -t > "${NAME}.json"
done
```

### Implement your "Query Performance Management"

To "manage" query performance means to measure, compare, and take action based
on the results. The previous examples have demonstrated how to measure query
performance. If you compare these measurements against some evaluation criteria
and take appropriate actions, you will have completed the cycle of query
performance management.

For example, you could compare the cost values in the JSON output generated
during the previous build with the current results. If you detect a query whose
cost has significantly increased, you could configure your CI system to fail the
build. This way, query performance management becomes an automated and
actionable part of your development workflow.

## Database Performance Management

Database performance management is achieved by taking into account the number of
query executions in actual workloads, in addition to the query performance
management described. Conversely, selecting queries with a high execution count
in actual workloads as targets for query performance management is also a
practical strategy for database performance management.

To obtain information on query usage in actual workloads, it is necessary to use
database query logs, application execution logs, and other such log information.
As an alternative to relying on logs, one can periodically sample the currently
running queries from PostgreSQL session information. The former approach
(provided there are no missing log entries) allows you to obtain the exact
number of executions, while the latter efficiently picks up queries that have
relatively long execution times or are executed frequently.

### Application execution logs

`sql-fun` is a generator for application SQL execution code, but it does not
have a built-in feature to automatically output logs. However, it does provide
hooks where logging can be implemented. In fact, under the example directory,
there is code like the following:

```rust
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
```

By using it in combination with the
[instrument](https://docs.rs/tracing/latest/tracing/attr.instrument.html) macro
from the `tracing` crate, the Rust function name is reliably included in the
logs.

### Implement your "Database Performance Management"

Since the way application logs are collected and managed is unique to each case,
it is not possible to provide a concrete example of how to calculate the actual
execution count. However, all the necessary components are in place.

To reiterate:

In this context, "manage" means to measure, compare, and take action based on
the results.
