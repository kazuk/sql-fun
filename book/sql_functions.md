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

## Legacy section links

The links below preserve earlier section URLs and lead to the canonical
reference. The old [sql_query_one page](sql_functions/sql_query_one.md) also
provides navigation to these shared contracts.

<!-- Legacy fragment IDs require explicit HTML anchors. -->
<!-- rumdl-disable MD033 -->
<a id="postgresql-named-function-arguments"></a>

[postgresql named function arguments](sql-analysis/expressions/function-calls.md#postgresql-named-function-arguments)

<a id="attribute-arguments"></a>

[attribute arguments](sql_functions/attributes.md)

<a id="attribute-argument-examples"></a>

[attribute argument examples](sql_functions/attributes.md)

<a id="function-signature-requirements"></a>

[function signature requirements](sql_functions/signatures.md)

<a id="additional-requirements-for-sql_query_many"></a>

[additional requirements for sql_query_many](sql_functions/signatures.md#collecting-multiple-rows)

<a id="rowtype-requirements"></a>

[rowtype requirements](sql_functions/signatures.md#rowtype-requirements)

<a id="error-type-requirements"></a>

[error type requirements](sql_functions/signatures.md#error-type-requirements)

<a id="collector-type"></a>

[collector type](sql_functions/signatures.md#collector-type)

<a id="parameter-binding"></a>

[parameter binding](sql_functions/parameter-binding.md)

<a id="binding-from-a-dto"></a>

[binding from a dto](sql_functions/parameter-binding.md#binding-from-a-dto)

<a id="conditional-sql"></a>

[conditional sql](sql_functions/conditional-sql.md)

<a id="match-arms"></a>

[match arms](sql_functions/conditional-sql.md)

<a id="the-cond-attribute"></a>

[the cond attribute](sql_functions/conditional-sql.md)

<a id="internal-function-naming"></a>

[internal function naming](sql_functions/conditional-sql.md)

<a id="conditional-sql-example"></a>

[conditional sql example](sql_functions/conditional-sql.md)

<a id="aggregate-entity-fetch-example-for-sql-fun"></a>

[aggregate entity fetch example for sql fun](examples/aggregate-fetch.md)

<a id="example-fetching-an-aggregate-with-multiple-child-tables"></a>

[example fetching an aggregate with multiple child tables](examples/aggregate-fetch.md)

<a id="row-types"></a>

[row types](examples/aggregate-fetch.md)

<a id="root-query-select-the-parent-set-once"></a>

[root query select the parent set once](examples/aggregate-fetch.md)

<a id="child-queries-reuse-the-root-selection-as-a-cte"></a>

[child queries reuse the root selection as a cte](examples/aggregate-fetch.md)

<a id="collector-accumulate-rows-and-merge-at-the-end"></a>

[collector accumulate rows and merge at the end](examples/aggregate-fetch.md)

<a id="putting-it-together"></a>

[putting it together](examples/aggregate-fetch.md)

<!-- rumdl-enable MD033 -->
