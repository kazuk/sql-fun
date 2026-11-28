# pg_walinspect

Functions for inspecting write-ahead log records.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.1 | Initializes. Example resolves. |
| 19 snapshot | 1.1 | Initializes. Example resolves. |

## SQL example

Include `"pg_read_server_files"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT start_lsn FROM pg_get_wal_records_info('0/0'::pg_lsn, '0/1'::pg_lsn);
```

## Analysis boundaries

Analysis resolves the function and result columns. It does not read WAL, check
LSN availability, or verify runtime privileges.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgwalinspect.html)
