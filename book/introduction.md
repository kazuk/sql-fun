# Introduction

`sql-fun` turns SQL queries and statements into Rust functions.

It assumes that you **can write SQL and want to write SQL**.

It doesn't try to hide SQL behind abstractions or pretend it knows better than
your database. It just makes **Rust and SQL fit together**, cleanly, safely, and
enjoyably.

If you're looking for an ORM: this isn't it.  
If you're writing raw SQL and wish Rust could help more: this is it.

## What is `sql-fun`

`sql-fun` is a code generator for executing SQL queries and statements.

- It runs entirely at **compile time** — there is no runtime component.
- Generated functions use your application's database driver and supporting
  dependencies; `sql-fun` adds no runtime library of its own.
- It is designed for `PostgreSQL` and `SQLite`:
  - `PostgreSQL` dialect
    - SQL is parsed using `pg_query`, a full `PostgreSQL` parser.
      - Parser acceptance and semantic support are separate; see
        [SQL analysis coverage and limitations](sql-analysis.md).
    - `sql-fun check` analyzes selected application SQL against the configured
      catalog metadata before generated execution code is built.
  - `SQLite` dialect
    - SQL is parsed using `turso_parser`. SQLite support is under development
      and inherits [parser limitations](sql-analysis.md#sqlite-parser-limitations).
- It does not try to model your entire database schema.
  - Instead, it focuses only on what your application actually *does*—the SQL
    queries it runs.
  - This keeps the tool lightweight, focused, and reliable.
- It generates code compatible with `tokio-postgres` or `rusqlite`, the primary
  runtime `sql-fun` targets.

![sql-fun dataflow](./datafrow.svg)

## Schema-aware validation

Just place a schema dump file (e.g., `schema.develop.sql`) in the same folder as
your Cargo.toml.

```console
pg_dump ... -s > schema.develop.sql
```

`sql-fun` will validate your SQL against the schema—ensuring all referenced
tables and columns actually exist.

If you use multiple schemas (e.g., dev vs prod),

you can switch between them by setting the `SQL_FUN_SCHEMA` environment
variable.

For example:

```console
SQL_FUN_SCHEMA=prod sql-fun check
```

will use schema.prod.sql instead.

## Reusable CTE

Reusable SQL definitions can be stored in a [CTE Catalog](reusable_cte.md).
The catalog defines SQL text and references to other catalog entries. It does
not obtain query definitions from Rust function names.

### Reusable query definition in CTE catalog

```toml
sql_fun_cte_catalog_format_version = "1"
sql_dialect = "PostgreSQL"

[cte.select_all_active_users]
query = """
select u.id, u.name from users u where u.is_active = TRUE
"""
```

See the [CTE Catalog reference](reusable_cte.md) for formats, expansion and
query macro integration.

## CRUD table generation

`sql-fun check` generates an inspectable TOML inventory showing which selected
Rust query functions access each SQL-visible relation and column, and how.
It writes `sql-fun.<datasource>.<schema>.toml` beside the existing lockfile.
For datasource `default` and schema-dump variant `develop`, the filename is
`sql-fun.default.develop.toml`.

For example, the maintained PostgreSQL E2E fixture contains this query:

```sql
UPDATE e2e_items
SET id = id, name = ${name:text}
WHERE id = ${id:bigint}
RETURNING id, name
```

Its inventory query record looks like this (excerpt from the tested fixture):

```toml
{{#include ../tests/postgres-e2e/inventory.toml:update-query}}
```

The source fields preserve the extracted Rust location; `<unknown>` means the
extractor did not supply a file name. The relation groups its responsible Rust
queries by access purpose, with function and source identity directly available:

```toml
{{#include ../tests/postgres-e2e/inventory.toml:relation-accesses}}
```

For `public.e2e_items.id`, each of the four update purposes has its own query
list. Each reference includes the one-based SQL statement number within the
function:

```toml
{{#include ../tests/postgres-e2e/inventory.toml:update-id-accesses}}
```

`UpdateTarget` identifies the assignment destination, `UpdateInput` the value
expression read, `UpdateFilter` the WHERE read, and `UpdateReturning` the output
read. The relation also has `UpdateRows` involvement. Reads and writes to the
same column remain separate facts. INSERT, SELECT and DELETE retain their own
detailed purposes, including RETURNING and filtering.

The inventory covers the invocation's **selected targets**, not necessarily the
entire workspace. Regeneration replaces entries for touched datasources; it
leaves other datasource files untouched. Queries with no object accesses still
appear. View and table-function accesses stop at their SQL-visible objects;
view definitions and function bodies are not recursively expanded.

Use this data for schema-impact analysis or query-usage reports. It does not
certify access-control correctness or map queries to business functions.
Executable SQL and code-generation metadata remain in the lockfile. See
[`check`](cli/check.md#crudquery-inventory) for the complete field definitions,
validation options and failure behavior.

### Rendering a CRUD matrix

The inventory is organized for the developer's question: which Rust queries
access this relation or column, and for which purpose? Detailed purposes remain
visible rather than being collapsed into four letters. A report can enumerate
the access-kind → responsible-query-list mappings directly.

After converting TOML to JSON, this ordinary Handlebars template produces a
Markdown matrix. It uses only nested iteration and field access; it does not
interpret SQL, resolve numeric IDs or reshape the data:

```handlebars
{{#include ../tests/postgres-e2e/inventory.hbs}}
```

The beginning of the rendered matrix from the same tested fixture is:

{{#include ../tests/postgres-e2e/inventory.md:3:10}}

Use the column-access rows in the same template for a column matrix. A renderer
may choose a different presentation while consuming the same nested mappings.
