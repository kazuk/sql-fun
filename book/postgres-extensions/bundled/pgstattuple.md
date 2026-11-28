# pgstattuple

Functions exposing physical statistics about tables and indexes.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.5 | Initializes. Example resolves. |
| 18.1 | 1.5 | Initializes. Example resolves. |
| 19 snapshot | 1.5 | Initializes. Example resolves. |

## SQL example

Include `"pg_stat_scan_tables"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT table_len FROM pgstattuple(NULL::regclass);
```

## Analysis boundaries

Analysis resolves the statistics columns. It does not scan the relation or
evaluate runtime access privileges.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgstattuple.html)
