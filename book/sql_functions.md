# SQL Functions

The query macros turn SQL into callable Rust functions using the connection
you pass. Choose the macro by the result your application needs:

| Macro | Choose when | Successful result |
| --- | --- | --- |
| `sql_query_one` | Exactly one row must exist. | `Row`; zero/multiple rows are errors. |
| `sql_query_opt` | A missing row is expected. | `Option<Row>`; multiple rows are errors. |
| `sql_query_many` | Process any number of rows with your own handler. | Final collector, including for zero rows. |
| `sql_statement` | Execute without collecting result rows. | Affected-row count (`u64` for PostgreSQL, `usize` for SQLite). |

All return `Result<Success, Error>`. PostgreSQL (`tokio-postgres`) functions are
async; SQLite (`rusqlite`, with the `sqlite` feature) functions are synchronous.
Row-building queries use your row builder, and multi-row queries use your
collector and handler. The optional `sql-fun-collector` component is one choice
for collection; hand-written collectors are supported too.

- [Attributes](sql_functions/attributes.md): inline/files/structured SQL,
  datasource and builder options.
- [Function signatures](sql_functions/signatures.md): connections, transactions,
  return inference, row builders, errors and collector/handler contracts.
- [Parameter binding](sql_functions/parameter-binding.md): `${name}` and DTOs.
- [Conditional SQL](sql_functions/conditional-sql.md): statically expanded
  fragments selected by Rust match expressions.
- [Query interruption](sql_functions/query-interruption.md): connector-specific
  futures/channels, error contracts and cancellation limits.
- [Aggregate Fetch without sql-fun-collector](examples/aggregate-fetch.md):
  complete hand-written parent/child collection example.

## Reusable CTE integration

[Reusable CTE / CTE Catalog](reusable_cte.md) describes SQL catalog definitions,
nested expansion and placeholder mapping. A Rust query function's name is not a
source of reusable SQL. Query macros expand explicitly referenced Catalog
entries internally; ordinary SQL `WITH` clauses can also be written directly.

See [SQL analysis coverage and limitations](sql-analysis.md) for supported SQL
forms and diagnostics, and [installation](installation.md) for metadata and
lockfile preparation.
