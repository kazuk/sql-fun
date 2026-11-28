# pg_logicalinspect

Functions for examining logical-decoding snapshots.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

Include `"pg_read_server_files"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT state FROM pg_get_logical_snapshot_info(NULL::text);
```

## Analysis boundaries

Analysis resolves the returned columns. It does not open snapshot files or
validate their contents or runtime privileges.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pglogicalinspect.html)
