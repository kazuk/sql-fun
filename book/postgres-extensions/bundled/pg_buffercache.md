# pg_buffercache

Views and functions exposing information about shared buffers. Version `1.7`
in the PostgreSQL 19 snapshot adds the `pg_buffercache_os_pages` view.
Offline analysis resolves its `bufferid` and `os_page_num` columns.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.5 | Initializes. Example resolves. |
| 18.1 | 1.6 | Initializes. Example resolves. |
| 19 snapshot | 1.7 | Initializes. Example resolves. |

## SQL example

Include `"pg_monitor"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT bufferid FROM pg_buffercache;
```

## Analysis boundaries

Analysis resolves the view and its columns. Buffer occupancy, eviction, and
access privileges require a live PostgreSQL server.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgbuffercache.html)
