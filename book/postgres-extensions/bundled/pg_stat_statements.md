# pg_stat_statements

Views and functions exposing tracked statement statistics.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.11 | Initializes. Example resolves. |
| 18.1 | 1.12 | Initializes. Example resolves. |
| 19 snapshot | 1.13 | Initializes. Example resolves. |

## SQL example

Include `"pg_read_all_stats"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT queryid, calls FROM pg_stat_statements;
```

## Analysis boundaries

Analysis resolves the statistics view and its columns. It does not check
shared preloading, privileges, or how statements are tracked at runtime.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgstatstatements.html)
