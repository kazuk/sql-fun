# pg_freespacemap

Functions exposing available space recorded for relation pages.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.3 | Initializes. Example resolves. |
| 19 snapshot | 1.3 | Initializes. Example resolves. |

## SQL example

Include `"pg_stat_scan_tables"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT pg_freespace(NULL::regclass, 0);
```

## Analysis boundaries

Analysis resolves the function's signature. It does not inspect relation
blocks or determine the amount of recorded free space.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgfreespacemap.html)
