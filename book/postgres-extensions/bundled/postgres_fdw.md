# postgres_fdw

The `postgres_fdw` wrapper and functions for inspecting or closing its
connections.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.3 | Default fails to initialize. |
| 19 snapshot | 1.2 | Explicit selection initializes. Example resolves. |

## SQL example

For the PostgreSQL 19 catalog, explicitly select `postgres_fdw@1.2`.

```sql
SELECT postgres_fdw_handler();
```

## Analysis boundaries

Analysis resolves declared wrapper functions without connecting to remote
servers, checking credentials or wrapper options at runtime, or loading a
remote schema.

The CLI rejects standalone `CREATE SERVER` as unsupported.
[Tracking issue #1306](https://github.com/kazuk/sql-fun/issues/1306).

On the PostgreSQL 19 snapshot, the default `1.3` upgrade stops at
`ALTER FOREIGN DATA WRAPPER … CONNECTION` during SQL splitting.
With `1.2` pinned, the example resolves and
`SELECT postgres_fdw_disconnect_all();` resolves to `pg_catalog.bool`.
[Tracking issue #1309](https://github.com/kazuk/sql-fun/issues/1309).

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/postgres-fdw.html)
